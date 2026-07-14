mod alias;
mod binding;
mod provenance;
mod seed;
mod shape;

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

        if let Some(name) = self.loop_header_explicit_merge_binding_name(loop_head, output) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output, proof) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_loop_carried_input_name(op, output, proof) {
            return Some(name);
        }
        if let Some(name) =
            self.loop_header_external_seed_binding_name_for_update(block_idx, output)
        {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_same_register_output_name(output) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            return Some(name);
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
        }
        // Confirmed loop-carried update without a prior materialized seed still needs a
        // stable identity. Without it, RHS self-reads inline the pre-loop def
        // (e.g. xor-zero → Const(0) so `edx = edx + ecx` becomes `edx = ecx`) and
        // stack-loaded induction registers get a fresh temp instead of `param_N`
        // (so `shr` never writes back into the value the next iteration reads).
        if let Some(name) = self.loop_carried_stack_param_seed_name(block_idx, output) {
            return Some(name);
        }
        if let Some(name) = self.loop_carried_hardware_register_name(output) {
            return Some(name);
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
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_loop_carried_input_name(op, output, proof) {
            return Some(name);
        }
        if let Some(name) =
            self.loop_header_external_seed_binding_name_for_update(block_idx, output)
        {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_same_register_output_name(output) {
            return Some(name);
        }
        if let Some(name) = self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            return Some(name);
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
        }
        if let Some(name) = self.loop_carried_stack_param_seed_name(block_idx, output) {
            return Some(name);
        }
        if let Some(name) = self.loop_carried_hardware_register_name(output) {
            return Some(name);
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

    fn stack_param_name_reaching_register(&mut self, output: &Varnode) -> Option<String> {
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

    fn stack_param_name_from_load_op(&mut self, op: &PcodeOp) -> Option<String> {
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
