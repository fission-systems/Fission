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
        ) || self.abi_state().param_slot_for_varnode(output).is_none()
        {
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
                    || Self::has_aliasing_side_effect_between_ops(pred_block, def_idx + 1, term_idx)
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
