use super::*;

impl<'a> PreviewBuilder<'a> {
    /// Reuse the binding at the first proven merge reached by a loop-carried
    /// definition.  A loop update is not a one-iteration value that can be
    /// folded into a flat `Select`; it is the state variable that must keep
    /// the merge's name across every iteration.  The ordinary merge policy
    /// intentionally declines non-direct successors for that reason.  This
    /// path is narrower: the caller has already proved the definition is
    /// loop-carried, and the merge proof must still establish complete,
    /// conflicting, scalar incoming values.
    pub(super) fn merge_binding_name_for_loop_carried_output(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> Option<String> {
        let rhs_kind = Self::loop_carried_merge_rhs_kind(op.opcode)?;
        let proof = self.describe_merge_binding_candidate_proof_with_rhs_kind(
            block, op_idx, output, rhs_kind,
        )?;
        let duplicate_start = self.duplicate_start_merge_block(proof.merge_block);
        if !self.merge_binding_proof_allows_predecessor_assignment(&proof, duplicate_start) {
            return None;
        }
        let block_idx = self.lowering_block_index(block);
        let (merge_idx, merge_addr, _, _) =
            self.first_output_use_site_outside_block_by_index(block_idx, op_idx, output)?;
        if merge_addr != proof.merge_block
            || !self
                .prove_definition_reaches_block_entry(block_idx, op_idx, output, merge_idx)
                .is_some()
        {
            return None;
        }
        let binding = self.ensure_explicit_merge_binding_for_block(merge_idx, output);
        self.trace_explicit_merge_binding_trial(
            proof.merge_block,
            output,
            &[],
            &[],
            &proof.incoming_value_kinds,
            proof.rhs_kind,
            &binding.name,
            true,
            ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
        );
        Some(binding.name)
    }

    fn loop_carried_merge_rhs_kind(opcode: PcodeOpcode) -> Option<DisallowedSingleConsumerRhsKind> {
        match opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => Some(DisallowedSingleConsumerRhsKind::VarOrConst),
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::PopCount
            | PcodeOpcode::LzCount => Some(DisallowedSingleConsumerRhsKind::Arithmetic),
            _ => None,
        }
    }

    pub(super) fn merge_binding_name_for_materialized_output(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> Option<String> {
        let block_idx = self.lowering_block_index(block);
        let key = VarnodeKey::from(output);
        for succ_idx in self.successors.get(block_idx)? {
            let Some(reach_proof) =
                self.prove_definition_reaches_block_entry(block_idx, op_idx, output, *succ_idx)
            else {
                continue;
            };
            debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
            debug_assert_eq!(reach_proof.target_block(), *succ_idx);
            if let Some(name) = self.explicit_merge_bindings.get(&(*succ_idx, key.clone())) {
                return Some(name.clone());
            }
        }
        if let Some(name) =
            self.merge_binding_name_for_direct_successor_accumulator(block, op_idx, output, rhs)
        {
            return Some(name);
        }

        let proof = self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)?;
        let duplicate_start = self.duplicate_start_merge_block(proof.merge_block);
        if !self.merge_binding_proof_allows_predecessor_assignment(&proof, duplicate_start) {
            return None;
        }
        let (merge_idx, merge_addr, _, _) =
            self.first_output_use_site_outside_block_by_index(block_idx, op_idx, output)?;
        let reach_proof =
            self.prove_definition_reaches_block_entry(block_idx, op_idx, output, merge_idx)?;
        debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
        debug_assert_eq!(reach_proof.target_block(), merge_idx);
        if merge_addr == proof.merge_block
            && let Some(name) = self.explicit_merge_bindings.get(&(merge_idx, key.clone()))
        {
            return Some(name.clone());
        }
        if merge_addr != proof.merge_block
            || !self
                .successors
                .get(block_idx)
                .is_some_and(|succs| succs.contains(&merge_idx))
        {
            return None;
        }
        let binding = self.ensure_explicit_merge_binding_for_block(merge_idx, output);
        self.trace_explicit_merge_binding_trial(
            proof.merge_block,
            output,
            &[],
            &[],
            &proof.incoming_value_kinds,
            proof.rhs_kind,
            &binding.name,
            true,
            ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
        );
        Some(binding.name)
    }

    pub(super) fn merge_binding_proof_allows_predecessor_assignment(
        &self,
        proof: &MergeBindingCandidateProof,
        duplicate_start: bool,
    ) -> bool {
        proof.can_synthesize_phi_like_binding
            && (proof.predecessor_count > 2 || (duplicate_start && proof.predecessor_count == 2))
            && proof.missing_incoming_count == 0
            && proof.conflicting_incoming_count >= 1
            && matches!(
                proof.consumer_kind,
                DisallowedSingleConsumerConsumerKind::OtherData
                    | DisallowedSingleConsumerConsumerKind::Predicate
            )
            && proof.incoming_value_kinds.iter().all(|kind| {
                matches!(
                    kind,
                    MergeBindingCandidateIncomingKind::VarOrConst
                        | MergeBindingCandidateIncomingKind::Arithmetic
                )
            })
    }

    pub(super) fn duplicate_start_merge_block(&self, merge_block: u64) -> bool {
        self.pcode
            .blocks
            .iter()
            .filter(|block| block.start_address == merge_block)
            .take(2)
            .count()
            >= 2
    }
}
