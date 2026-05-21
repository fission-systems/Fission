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
        if !Self::is_loop_carried_register_update_candidate(output) {
            return None;
        }
        let block_idx = self.address_to_index.get(&block.start_address).copied()?;
        if !self.output_is_loop_carried_register_update(block_idx, op_idx, op, output) {
            return None;
        }
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output) {
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
        if let Some(name) =
            self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            return Some(name);
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
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
        if !self.loop_bodies.iter().any(|loop_body| {
            loop_body.body.contains(&block_idx)
                && self.loop_update_reaches_backedge_tail(block_idx, loop_body)
                && (Self::op_reads_varnode_key(input_def, &output_key)
                    || self.loop_reads_varnode_before_update(
                        loop_body,
                        block_idx,
                        input_def_idx,
                        &output_key,
                    ))
        }) {
            return None;
        }
        if let Some(name) = self.prior_materialized_loop_carried_output_name(output) {
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
        if let Some(name) =
            self.prior_materialized_local_wide_alias_name(block_idx, op_idx, output)
        {
            return Some(name);
        }
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
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
