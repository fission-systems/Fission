//! Conservative proof rules for restarting a cross-block definition window.
//!
//! A definition may be materialized again at a later overwrite only when the
//! overwrite preserves the value or predicate family, dominates the eventual
//! consumer, and no alias-sensitive operation intervenes before the block's
//! terminator.  Keeping the proof and its diagnostics together makes this
//! opt-in materialization policy independent from the broader cross-block
//! consumer and merge analysis.

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder::materialize) fn copy_overwrite_restart_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            matches!(
                std::env::var("FISSION_ENABLE_COPY_OVERWRITE_RESTART"),
                Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
            )
        })
    }

    pub(in crate::midend::builder::materialize) fn predicate_refresh_restart_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            matches!(
                std::env::var("FISSION_ENABLE_PREDICATE_REFRESH_RESTART"),
                Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
            )
        })
    }

    pub(in crate::midend::builder::materialize) fn can_restart_def_window_at_copy_overwrite(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<CopyOverwriteRestartProof> {
        let restart_op_idx = Self::first_output_redefinition_in_block(block, op_idx, output)?.0;
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, restart_op_idx, output)?;
        if !matches!(
            provenance.relation,
            CrossBlockConsumerRelation::SuccessorBlock
                | CrossBlockConsumerRelation::PostDominatorBlock
        ) || provenance.consumer_is_multiequal
            || provenance.relation == CrossBlockConsumerRelation::LoopBackedge
        {
            return None;
        }
        let redef = self.describe_cross_block_redefinition_detail(
            block,
            op_idx,
            output,
            consumer_block_addr,
        )?;
        let proof = self.describe_copy_overwrite_restart_proof(block, op_idx, output, &redef)?;
        if !proof.same_value || !proof.redef_dominates_consumer || proof.old_def_has_pre_redef_use {
            return None;
        }
        if !Self::copy_overwrite_rhs_is_pure_restart_candidate(&redef) {
            return None;
        }
        if !Self::no_alias_hazard_between_redef_and_terminator(
            block,
            redef.redef_op_idx,
            terminator_index,
        ) {
            return None;
        }
        Some(CopyOverwriteRestartProof {
            consumer_relation: provenance.relation,
            ..proof
        })
    }

    pub(in crate::midend::builder::materialize) fn can_restart_def_window_at_predicate_refresh(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<PredicateOverwriteRefreshProof> {
        let restart_op_idx = Self::first_output_redefinition_in_block(block, op_idx, output)?.0;
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, restart_op_idx, output)?;
        if provenance.relation != CrossBlockConsumerRelation::PostDominatorBlock
            || provenance.consumer_is_multiequal
            || provenance.relation == CrossBlockConsumerRelation::LoopBackedge
        {
            return None;
        }
        let redef = self.describe_cross_block_redefinition_detail(
            block,
            op_idx,
            output,
            consumer_block_addr,
        )?;
        if !Self::predicate_refresh_rhs_is_restart_candidate(&redef) {
            return None;
        }
        let proof = self.describe_predicate_overwrite_refresh_proof(
            block,
            op_idx,
            output,
            &redef,
            provenance.relation,
        )?;
        if !proof.same_guard_family
            || proof.old_def_has_pre_redef_use
            || !proof.redef_dominates_predicate
        {
            return None;
        }
        let consumer_block_idx = self
            .address_to_index
            .get(&proof.predicate_consumer_block_addr)
            .copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let consumer_op = consumer_block
            .ops
            .iter()
            .find(|candidate| candidate.seq_num == proof.predicate_consumer_op_seq)?;
        if consumer_op.opcode != PcodeOpcode::BoolNegate {
            return None;
        }
        if !Self::no_alias_hazard_between_redef_and_terminator(
            block,
            redef.redef_op_idx,
            terminator_index,
        ) {
            return None;
        }
        Some(proof)
    }

    fn copy_overwrite_rhs_is_pure_restart_candidate(redef: &CrossBlockRedefinitionDetail) -> bool {
        matches!(redef.redef_rhs_kind, SameBlockOverwriteRhsKind::CopyLike)
            && matches!(
                redef.overwrite_shape,
                SameBlockOverwriteShapeKind::OverwriteAtCopy
            )
    }

    fn predicate_refresh_rhs_is_restart_candidate(redef: &CrossBlockRedefinitionDetail) -> bool {
        matches!(redef.redef_rhs_kind, SameBlockOverwriteRhsKind::Predicate)
            && matches!(
                redef.overwrite_shape,
                SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
            )
            && matches!(
                redef.redef_opcode,
                PcodeOpcode::IntEqual
                    | PcodeOpcode::IntNotEqual
                    | PcodeOpcode::BoolNegate
                    | PcodeOpcode::BoolXor
            )
    }

    fn no_alias_hazard_between_redef_and_terminator(
        block: &crate::pcode::PcodeBasicBlock,
        redef_idx: usize,
        terminator_index: Option<usize>,
    ) -> bool {
        let Some(term_idx) = terminator_index else {
            return false;
        };
        if redef_idx >= term_idx {
            return false;
        }
        !block.ops[redef_idx + 1..term_idx].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Store
                    | PcodeOpcode::Load
            )
        })
    }

    pub(in crate::midend::builder::materialize) fn describe_copy_overwrite_restart_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
    ) -> Option<CopyOverwriteRestartProof> {
        if redef.relation != CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
            || redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtCopy
        {
            return None;
        }
        let redef_op = block.ops.get(redef.redef_op_idx)?;
        let (consumer_block_addr, _, _) = self.first_output_use_site_outside_block(
            block.start_address,
            redef.redef_op_idx,
            output,
        )?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (_consumer_idx, consumer_op) =
            consumer_block
                .ops
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate
                        .inputs
                        .iter()
                        .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output))
                })?;
        let def_op = block.ops.get(op_idx)?;
        let old_def_has_pre_redef_use =
            !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty();
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let redef_dominates_consumer = self.dom_tree.dominates(def_block_idx, consumer_block_idx)
            && self
                .block_terminator_index(block)
                .is_some_and(|term_idx| redef.redef_op_idx < term_idx);
        Some(CopyOverwriteRestartProof {
            consumer_relation: CrossBlockConsumerRelation::UnreachableOrUnclassified,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs: Self::format_copy_overwrite_inputs(&redef_op.inputs),
            same_value: Self::ops_share_copylike_value(def_op, redef_op),
            redef_dominates_consumer,
            old_def_has_pre_redef_use,
            consumer_block_addr,
            consumer_op_seq: consumer_op.seq_num,
        })
    }

    pub(in crate::midend::builder::materialize) fn describe_predicate_overwrite_refresh_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_relation: CrossBlockConsumerRelation,
    ) -> Option<PredicateOverwriteRefreshProof> {
        if redef.relation != CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
            || redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
        {
            return None;
        }
        let redef_op = block.ops.get(redef.redef_op_idx)?;
        let (consumer_block_addr, _, _) = self.first_output_use_site_outside_block(
            block.start_address,
            redef.redef_op_idx,
            output,
        )?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (_consumer_idx, consumer_op) =
            consumer_block
                .ops
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate
                        .inputs
                        .iter()
                        .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output))
                })?;
        let old_def_has_pre_redef_use =
            !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty();
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let redef_dominates_predicate = self.dom_tree.dominates(def_block_idx, consumer_block_idx)
            && self
                .block_terminator_index(block)
                .is_some_and(|term_idx| redef.redef_op_idx < term_idx);
        Some(PredicateOverwriteRefreshProof {
            consumer_relation,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs: Self::format_copy_overwrite_inputs(&redef_op.inputs),
            predicate_consumer_block_addr: consumer_block_addr,
            predicate_consumer_op_seq: consumer_op.seq_num,
            predicate_rhs: Self::format_copy_overwrite_inputs(&consumer_op.inputs),
            same_guard_family: Self::predicate_consumer_matches_output_guard_family(
                consumer_op,
                output,
            ),
            old_def_has_pre_redef_use,
            redef_dominates_predicate,
        })
    }

    fn predicate_consumer_matches_output_guard_family(
        consumer_op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        match consumer_op.opcode {
            PcodeOpcode::BoolNegate => consumer_op
                .inputs
                .first()
                .is_some_and(|input| VarnodeKey::from(input) == key),
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual | PcodeOpcode::BoolXor => {
                if consumer_op.inputs.len() != 2 {
                    return false;
                }
                let lhs_matches = VarnodeKey::from(&consumer_op.inputs[0]) == key
                    && consumer_op.inputs[1].is_constant
                    && consumer_op.inputs[1].constant_val <= 1;
                let rhs_matches = VarnodeKey::from(&consumer_op.inputs[1]) == key
                    && consumer_op.inputs[0].is_constant
                    && consumer_op.inputs[0].constant_val <= 1;
                lhs_matches || rhs_matches
            }
            PcodeOpcode::CBranch => consumer_op
                .inputs
                .get(1)
                .is_some_and(|input| VarnodeKey::from(input) == key),
            _ => false,
        }
    }

    fn ops_share_copylike_value(def_op: &PcodeOp, redef_op: &PcodeOp) -> bool {
        matches!(
            redef_op.opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
        ) && def_op.opcode == redef_op.opcode
            && def_op.inputs == redef_op.inputs
    }

    pub(in crate::midend::builder::materialize) fn format_copy_overwrite_inputs(
        inputs: &[Varnode],
    ) -> String {
        let formatted = inputs
            .iter()
            .map(Self::format_copy_overwrite_varnode)
            .collect::<Vec<_>>()
            .join(",");
        format!("[{formatted}]")
    }

    fn format_copy_overwrite_varnode(vn: &Varnode) -> String {
        if vn.is_constant {
            format!("const(0x{:x}:s{})", vn.offset, vn.size)
        } else {
            format!("space:{}:0x{:x}:s{}", vn.space_id, vn.offset, vn.size)
        }
    }
}
