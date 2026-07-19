mod alias;
mod binding;
mod provenance;
mod seed;
mod shape;

pub(in crate::midend::builder) use shape::LoopCarriedDefinitionProof;

#[cfg(test)]
mod tests;

use super::*;
use std::collections::BTreeSet;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn loop_carried_output_binding_name(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> Option<String> {
        let is_candidate = Self::is_loop_carried_register_update_candidate(output);
        let block_idx = self.address_to_index.get(&block.start_address).copied();

        if !is_candidate {
            return None;
        }
        let block_idx = block_idx?;
        let proof = self.prove_loop_carried_register_update(block_idx, op_idx, output)?;
        debug_assert_eq!(proof.definition_site(), (block_idx, op_idx));
        let loop_head = proof.loop_head();

        // Non-anonymous merge / prior bindings keep their stable names first
        // (e.g. cursor register `edx` seeded from a pointer param must stay `edx`,
        // not be rewritten to `param_1++` while `*edx` still reads `edx`).
        if let Some(name) = self.loop_header_explicit_merge_binding_name(loop_head, output) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
            // Anonymous merge temps lose to stack-param / hardware identity below.
        }
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output, proof) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
            // Anonymous preheader seed: keep for non-primary-return GPRs so the
            // stride update reuses the seeded binding. Primary-return full
            // registers may still prefer stack-param identity below.
            let may_prefer_stack = output.size >= 4
                && self.register_namer().is_primary_return_register(output);
            if !may_prefer_stack {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_loop_carried_input_name(op, output, proof) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
            let may_prefer_stack = output.size >= 4
                && self.register_namer().is_primary_return_register(output);
            if !may_prefer_stack {
                return Some(name);
            }
        }
        if let Some(name) =
            self.loop_header_external_seed_binding_name_for_update(block_idx, output)
        {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_same_register_output_name(output) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
        }
        // Resolve stack-param seed vs hardware name when both exist:
        // - Primary return *full* register (x86 EAX holding a scalar stack arg
        //   that is also the induction, e.g. count_bits `shr`): prefer `param_N`
        //   so the entry test and `>>=` share one binding (anonymous `uVar` loses).
        // - Other GPRs (checksum cursor `edx++` seeded from a pointer param):
        //   prefer the hardware name so we do not emit `param_1++` while
        //   `*edx` still reads a distinct copy.
        // Partial primary-return lanes (AL): prefer an existing wide same-family
        // binding (xor-zero on EAX as `uVar1`/`eax`) over bare `al`, so the
        // accumulator reuses the zero seed (`sum += *p` not bare `al += *p`).
        //
        // Bare hardware must lose to a preheader-materialized seed (including
        // anonymous) so pointer-scan loops keep `seed = base; *seed; seed += n`
        // rather than an unbound `reg += n` after the seed binding was dropped.
        if output.size < 4 && self.register_namer().is_primary_return_register(output) {
            if let Some(wide) =
                self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
            {
                return Some(wide);
            }
            if let Some(wide) = self.wide_primary_return_materialized_name(output) {
                return Some(wide);
            }
        }
        let stack_seed = self.loop_carried_stack_param_seed_name(block_idx, output);
        let hw_name = self.loop_carried_hardware_register_name(output);
        let prefer_stack_over_hw =
            output.size >= 4 && self.register_namer().is_primary_return_register(output);
        match (stack_seed, hw_name) {
            (Some(stack), Some(hw)) if stack != hw && prefer_stack_over_hw => {
                return Some(stack);
            }
            (Some(_), Some(hw)) if !prefer_stack_over_hw => {
                // Prefer preheader seed (incl. anonymous) over bare hw.
                if let Some(name) =
                    self.prior_materialized_loop_carried_output_name(output, proof)
                {
                    return Some(name);
                }
                if let Some(name) =
                    self.prior_materialized_loop_carried_input_name(op, output, proof)
                {
                    return Some(name);
                }
                return Some(hw);
            }
            (Some(stack), _) => return Some(stack),
            (None, Some(hw)) => {
                // Same: seeded binding beats unbound hardware identity.
                if let Some(name) =
                    self.prior_materialized_loop_carried_output_name(output, proof)
                {
                    return Some(name);
                }
                if let Some(name) =
                    self.prior_materialized_loop_carried_input_name(op, output, proof)
                {
                    return Some(name);
                }
                return Some(hw);
            }
            (None, None) => {}
        }
        // Last resort: keep anonymous merge/prior temp if that is all we have.
        if let Some(name) = self.loop_header_explicit_merge_binding_name(loop_head, output) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output, proof) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_loop_carried_input_name(op, output, proof) {
            return Some(name);
        }
        None
    }

    fn is_anonymous_temp_binding_name(name: &str) -> bool {
        name.starts_with("uVar")
            || name.starts_with("xVar")
            || name.starts_with("iVar")
            || name.starts_with("bVar")
            || name.starts_with("tmp_")
    }

    /// Find a materialized name for a wider view of the primary return register
    /// (e.g. EAX after `xor eax,eax` when materializing an AL self-update).
    fn wide_primary_return_materialized_name(&self, output: &Varnode) -> Option<String> {
        if output.size >= 4 || !self.register_namer().is_primary_return_register(output) {
            return None;
        }
        let mut best: Option<(u32, String)> = None;
        for (key, name) in &self.materialized_vns {
            let vn = &key.varnode;
            if vn.is_constant || !is_register_space_id(vn.space_id) {
                continue;
            }
            if vn.space_id != output.space_id || vn.offset != output.offset {
                continue;
            }
            if vn.size <= output.size {
                continue;
            }
            // Same storage family as the primary return (offset match + register space).
            if name.starts_with("param_") {
                continue;
            }
            match &best {
                Some((sz, _)) if *sz >= vn.size => {}
                _ => best = Some((vn.size, name.clone())),
            }
        }
        best.map(|(_, name)| name)
    }

    /// When **reading** a register inside a natural loop that also **updates**
    /// that register on a backedge (pointer-scan / induction), use the same
    /// binding name the update will use.
    ///
    /// Without this, LOAD often resolves the preheader seed (`uVar = base`)
    /// while INT_ADD prefers a bare hardware register name, producing a frozen
    /// load base plus a distinct unbound cursor update (`*uVar` / `reg += n`).
    pub(in crate::midend::builder) fn loop_body_carried_register_read_name(
        &mut self,
        vn: &Varnode,
    ) -> Option<String> {
        if vn.is_constant || !is_register_space_id(vn.space_id) {
            return None;
        }
        if !Self::is_loop_carried_register_update_candidate(vn) {
            return None;
        }
        let site = self.current_lowering_site?;
        let site_block = self.pcode.blocks.get(site.block_idx)?;

        // Prefer the name of a loop-carried self-update when we are reading that
        // register as an input of the update op itself (eax = eax + 4).
        // current_lowering_site already matches the update definition site.
        if let Some(op) = site_block.ops.get(site.op_idx)
            && let Some(out) = op.output.as_ref()
            && (self.varnode_aliases_value(out, vn)
                || VarnodeKey::from(out) == VarnodeKey::from(vn))
            && self
                .prove_loop_carried_register_update(site.block_idx, site.op_idx, out)
                .is_some()
        {
            if let Some(name) =
                self.loop_carried_output_binding_name(site_block, site.op_idx, op, out)
            {
                return Some(name);
            }
        }

        // Otherwise, if some op in the same loop body updates `vn`, use that
        // update's binding for this read (LOAD [eax] before ADD eax, 4).
        // Must temporarily set current_lowering_site to the update op — binding
        // helpers debug_assert that site matches the carried definition.
        let loop_body_idxs: Vec<usize> = self
            .loop_bodies
            .iter()
            .filter(|lb| lb.body.contains(&site.block_idx))
            .min_by_key(|lb| lb.body.len())
            .map(|lb| lb.body.iter().copied().collect())
            .unwrap_or_default();
        if loop_body_idxs.is_empty() {
            return None;
        }

        for &bidx in &loop_body_idxs {
            let op_count = self.pcode.blocks.get(bidx).map(|b| b.ops.len()).unwrap_or(0);
            for op_idx in 0..op_count {
                if bidx == site.block_idx && op_idx == site.op_idx {
                    continue;
                }
                let (out_key, has_out) = {
                    let block = match self.pcode.blocks.get(bidx) {
                        Some(b) => b,
                        None => continue,
                    };
                    let op = match block.ops.get(op_idx) {
                        Some(o) => o,
                        None => continue,
                    };
                    match op.output.as_ref() {
                        Some(out) => (
                            VarnodeKey::from(out),
                            self.varnode_aliases_value(out, vn)
                                || VarnodeKey::from(out) == VarnodeKey::from(vn),
                        ),
                        None => continue,
                    }
                };
                if !has_out {
                    continue;
                }
                // Re-borrow for prove / name selection under the update's site.
                let name = self.with_lowering_site(
                    crate::midend::builder::LoweringSite {
                        block_idx: bidx,
                        op_idx,
                    },
                    |this| {
                        let block = this.pcode.blocks.get(bidx)?;
                        let op = block.ops.get(op_idx)?;
                        let out = op.output.as_ref()?;
                        if VarnodeKey::from(out) != out_key {
                            return None;
                        }
                        if this
                            .prove_loop_carried_register_update(bidx, op_idx, out)
                            .is_none()
                        {
                            return None;
                        }
                        this.loop_carried_output_binding_name(block, op_idx, op, out)
                    },
                );
                if let Some(name) = name {
                    return Some(name);
                }
            }
        }
        None
    }

    pub(super) fn loop_carried_passthrough_output_binding_name(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> Option<String> {
        if !Self::is_loop_carried_register_update_candidate(output)
            || !matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
        {
            return None;
        }
        let input = op.inputs.first()?;
        let input_key = VarnodeKey::from(input);
        let output_key = VarnodeKey::from(output);
        let block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let input_def_idx = block.ops.iter().take(op_idx).position(|candidate| {
            candidate
                .output
                .as_ref()
                .is_some_and(|candidate_output| VarnodeKey::from(candidate_output) == input_key)
        })?;
        let input_def = block.ops.get(input_def_idx)?;
        if !Self::op_reads_varnode_key(input_def, &output_key) {
            return None;
        }
        let proof = self.prove_loop_carried_register_update(block_idx, op_idx, output)?;
        debug_assert_eq!(proof.definition_site(), (block_idx, op_idx));
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output, proof) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_loop_carried_input_name(op, output, proof) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) =
            self.loop_header_external_seed_binding_name_for_update(block_idx, output)
        {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_same_register_output_name(output) {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if let Some(name) = self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            if !Self::is_anonymous_temp_binding_name(&name) {
                return Some(name);
            }
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
        }
        let stack_seed = self.loop_carried_stack_param_seed_name(block_idx, output);
        let hw_name = self.loop_carried_hardware_register_name(output);
        let prefer_stack_over_hw = self.register_namer().is_primary_return_register(output);
        match (stack_seed, hw_name) {
            (Some(stack), Some(hw)) if stack != hw && prefer_stack_over_hw => {
                return Some(stack);
            }
            (Some(_), Some(hw)) if !prefer_stack_over_hw => return Some(hw),
            (Some(stack), _) => return Some(stack),
            (None, Some(hw)) => return Some(hw),
            (None, None) => {}
        }
        None
    }

    fn loop_carried_output_has_prior_definition(&self, output: &Varnode) -> bool {
        let Some((_, op)) = self.lookup_def_site(output) else {
            return false;
        };
        let Some(prior_output) = op.output.as_ref() else {
            return false;
        };
        VarnodeKey::from(prior_output) == VarnodeKey::from(output)
    }

    /// Prefer the formal parameter name when a loop-carried register is seeded by
    /// loading an incoming stack argument (x86-32 cdecl/stdcall).
    fn loop_carried_stack_param_seed_name(
        &mut self,
        block_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        if !is_register_space_id(output.space_id) || output.is_constant {
            return None;
        }
        let loop_body = self
            .loop_bodies
            .iter()
            .find(|body| body.body.contains(&block_idx))?;
        let header_preds = self.predecessors.get(loop_body.head)?;
        let external_preds = header_preds
            .iter()
            .copied()
            .filter(|pred_idx| !loop_body.body.contains(pred_idx))
            .collect::<Vec<_>>();
        if external_preds.is_empty() {
            return None;
        }

        let mut names = BTreeSet::new();
        for pred_idx in external_preds {
            let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
                return None;
            };
            let term_idx = self
                .block_terminator_index(pred_block)
                .unwrap_or(pred_block.ops.len());
            let Some((_, pred_op)) =
                self.last_register_redefinition_before(pred_block, term_idx, output)
            else {
                // No redefinition on this external edge: try the entry-block load that
                // originally fed the register (common when a nop/align block sits
                // between the load and the loop head).
                if let Some(name) = self.stack_param_name_reaching_register(output) {
                    names.insert(name);
                    continue;
                }
                return None;
            };
            if let Some(name) = self.stack_param_name_from_register_defining_op(pred_op, output) {
                names.insert(name);
            } else if let Some(name) = self.stack_param_name_reaching_register(output) {
                names.insert(name);
            } else {
                return None;
            }
        }
        if names.len() == 1 {
            names.into_iter().next()
        } else {
            None
        }
    }

    /// Walk a short Copy/ZExt chain to an incoming stack-parameter Load.
    /// Used by loop-carried seeds and by x86_32 CallInd staged-arg recovery so
    /// later redefs of EAX cannot rewrite a frozen param snapshot.
    pub(in crate::midend::builder) fn stack_param_name_reaching_register(
        &mut self,
        output: &Varnode,
    ) -> Option<String> {
        // Walk a short chain of register-defining ops outside the current site to
        // find a stack-parameter load that feeds this register at function entry.
        let mut current = output.clone();
        for _ in 0..6 {
            let Some((site, op)) = self.lookup_def_site(&current) else {
                return None;
            };
            if let Some(name) = self.stack_param_name_from_register_defining_op(op, &current) {
                return Some(name);
            }
            match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => {
                    let input = op.inputs.first()?.clone();
                    if input.is_constant || input == current {
                        return None;
                    }
                    // Only continue if the def is not the loop-carried update itself.
                    if let Some(lowering) = self.current_lowering_site
                        && site.block_idx == lowering.block_idx
                        && site.op_idx >= lowering.op_idx
                    {
                        return None;
                    }
                    current = input;
                }
                _ => return None,
            }
        }
        None
    }

    fn stack_param_name_from_register_defining_op(
        &mut self,
        op: &PcodeOp,
        expected_output: &Varnode,
    ) -> Option<String> {
        let produced = op.output.as_ref()?;
        if !self.varnode_aliases_value(produced, expected_output)
            && VarnodeKey::from(produced) != VarnodeKey::from(expected_output)
        {
            return None;
        }
        match op.opcode {
            PcodeOpcode::Load => self.stack_param_name_from_load_op(op),
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                let input = op.inputs.first()?;
                let (_, src_op) = self.lookup_def_site(input)?;
                if src_op.opcode == PcodeOpcode::Load {
                    self.stack_param_name_from_load_op(src_op)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(in crate::midend::builder) fn stack_param_name_from_load_op(
        &mut self,
        op: &PcodeOp,
    ) -> Option<String> {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return None;
        }
        let addr = &op.inputs[1];
        let (base, offset) = self.resolve_stack_address(addr)?;
        let origin = self.classify_stack_slot_origin(base, offset);
        let NirBindingOrigin::ParamIndex(index) = origin else {
            return None;
        };
        let ty = op
            .output
            .as_ref()
            .map(|out| type_from_size(out.size, false))
            .unwrap_or_else(|| type_from_size(self.options.pointer_size, false));
        Some(self.ensure_incoming_stack_param_binding(index, ty).0)
    }

    fn loop_carried_hardware_register_name(&self, output: &Varnode) -> Option<String> {
        if output.is_constant || !is_register_space_id(output.space_id) {
            return None;
        }
        // Do not steal ABI register-parameter names; those are handled above.
        if self.abi_state().param_slot_for_varnode(output).is_some() {
            return None;
        }
        let name = self.sla_hw_name(output.offset, output.size)?;
        // Avoid colliding with an existing formal parameter binding that happens
        // to share a hardware name (should not occur for GPRs, but stay safe).
        if self.params.values().any(|binding| binding.name == name) {
            return None;
        }
        Some(name)
    }

    pub(super) fn bind_materialized_output_to_existing_name(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
        name: &str,
        preserve_materialization: bool,
    ) {
        self.materialized_vns
            .insert(MaterializedVarnodeKey::new(output, op), name.to_string());
        self.invalidate_materialization_dependent_caches();
        if preserve_materialization
            && let Some(binding) = self.temps.get_mut(name)
            && !binding.preserves_materialization()
            && binding.is_temp_like()
        {
            binding.origin = Some(NirBindingOrigin::TempPreserved);
            self.telemetry
                .materialization
                .materialization_stabilized_count += 1;
        }
    }
}
