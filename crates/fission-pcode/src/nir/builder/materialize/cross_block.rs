use super::contracts::*;
use super::*;
use std::collections::BTreeSet;
use std::collections::HashMap;

impl<'a> PreviewBuilder<'a> {
    fn best_prior_definition_for_missing_pred(
        &self,
        pred_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, u64, usize, u32, bool, usize, String)> {
        let mut best_prior_def: Option<(usize, u64, usize, u32, bool, usize, String)> = None;
        for candidate_idx in 0..self.pcode.blocks.len() {
            if candidate_idx == pred_idx {
                continue;
            }
            let Some(candidate_block) = self.pcode.blocks.get(candidate_idx) else {
                continue;
            };
            let Some((candidate_op_idx, candidate_op)) =
                Self::first_output_redefinition_in_block_from(candidate_block, 0, output)
            else {
                continue;
            };
            if !self.block_can_reach(candidate_idx, pred_idx, pred_idx) {
                continue;
            }
            let distance = self
                .shortest_forward_distance(candidate_idx, pred_idx)
                .unwrap_or(usize::MAX);
            let dominates = self.dom_tree.dominates(candidate_idx, pred_idx);
            let candidate = (
                candidate_idx,
                candidate_block.start_address,
                candidate_op_idx,
                candidate_op.seq_num,
                dominates,
                distance,
                Self::format_incoming_value(candidate_op),
            );
            let replace = match &best_prior_def {
                None => true,
                Some(current) => {
                    (candidate.4 && !current.4)
                        || (candidate.4 == current.4 && candidate.5 < current.5)
                        || (candidate.4 == current.4
                            && candidate.5 == current.5
                            && candidate.1 < current.1)
                }
            };
            if replace {
                best_prior_def = Some(candidate);
            }
        }
        best_prior_def
    }

    fn block_lies_on_path_slice(
        &self,
        start_idx: usize,
        block_idx: usize,
        target_idx: usize,
    ) -> bool {
        self.block_can_reach(start_idx, block_idx, target_idx)
            && self.block_can_reach(block_idx, target_idx, target_idx)
    }

    fn path_slice_has_output_redefinition(
        &self,
        start_idx: usize,
        start_op_idx: Option<usize>,
        target_idx: usize,
        output: &Varnode,
    ) -> bool {
        let Some(start_block) = self.pcode.blocks.get(start_idx) else {
            return false;
        };
        let start_search_idx = start_op_idx.map_or(0, |idx| idx + 1);
        if Self::first_output_redefinition_in_block_from(start_block, start_search_idx, output)
            .is_some()
        {
            return true;
        }
        for idx in 0..self.pcode.blocks.len() {
            if idx == start_idx || idx == target_idx {
                continue;
            }
            if !self.block_lies_on_path_slice(start_idx, idx, target_idx) {
                continue;
            }
            let Some(block) = self.pcode.blocks.get(idx) else {
                continue;
            };
            if Self::first_output_redefinition_in_block_from(block, 0, output).is_some() {
                return true;
            }
        }
        false
    }

    fn path_slice_crosses_call_or_store(
        &self,
        start_idx: usize,
        start_op_idx: Option<usize>,
        target_idx: usize,
    ) -> bool {
        let Some(start_block) = self.pcode.blocks.get(start_idx) else {
            return false;
        };
        let start_search_idx = start_op_idx.map_or(0, |idx| idx + 1);
        if start_block.ops.iter().skip(start_search_idx).any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Store
            )
        }) {
            return true;
        }
        for idx in 0..self.pcode.blocks.len() {
            if idx == start_idx || idx == target_idx {
                continue;
            }
            if !self.block_lies_on_path_slice(start_idx, idx, target_idx) {
                continue;
            }
            let Some(block) = self.pcode.blocks.get(idx) else {
                continue;
            };
            if block.ops.iter().any(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::CallOther
                        | PcodeOpcode::Store
                )
            }) {
                return true;
            }
        }
        false
    }

    fn shortest_forward_distance(&self, start_idx: usize, target_idx: usize) -> Option<usize> {
        if start_idx == target_idx {
            return Some(0);
        }
        let mut queue = std::collections::VecDeque::from([(start_idx, 0usize)]);
        let mut visited = HashSet::from([start_idx]);
        while let Some((idx, distance)) = queue.pop_front() {
            if let Some(succs) = self.successors.get(idx) {
                for succ in succs {
                    if !visited.insert(*succ) {
                        continue;
                    }
                    if *succ == target_idx {
                        return Some(distance + 1);
                    }
                    queue.push_back((*succ, distance + 1));
                }
            }
        }
        None
    }

    fn nearest_forward_join_block(&self, start_idx: usize) -> Option<(usize, usize)> {
        let mut queue = std::collections::VecDeque::from([(start_idx, 0usize)]);
        let mut visited = HashSet::from([start_idx]);
        while let Some((idx, distance)) = queue.pop_front() {
            if distance > 0 && self.predecessors.get(idx).map_or(0, Vec::len) > 1 {
                return Some((idx, distance));
            }
            if let Some(succs) = self.successors.get(idx) {
                for succ in succs {
                    if visited.insert(*succ) {
                        queue.push_back((*succ, distance + 1));
                    }
                }
            }
        }
        None
    }

    fn nearest_postdom_join_block(&self, start_idx: usize) -> Option<(usize, usize)> {
        let succs = self.successors.get(start_idx)?;
        if succs.len() < 2 {
            return None;
        }
        let candidate = self
            .cfg_facts
            .immediate_postdominators()
            .nearest_common_postdominator(succs)?;
        let distance = self.shortest_forward_distance(start_idx, candidate)?;
        Some((candidate, distance))
    }

    fn shortest_forward_path(&self, start_idx: usize, target_idx: usize) -> Option<Vec<usize>> {
        if start_idx == target_idx {
            return Some(vec![start_idx]);
        }
        let mut queue = std::collections::VecDeque::from([start_idx]);
        let mut parents = std::collections::HashMap::new();
        let mut visited = HashSet::from([start_idx]);
        while let Some(idx) = queue.pop_front() {
            if let Some(succs) = self.successors.get(idx) {
                for succ in succs {
                    if !visited.insert(*succ) {
                        continue;
                    }
                    parents.insert(*succ, idx);
                    if *succ == target_idx {
                        let mut path = vec![target_idx];
                        let mut cursor = target_idx;
                        while let Some(parent) = parents.get(&cursor).copied() {
                            path.push(parent);
                            if parent == start_idx {
                                break;
                            }
                            cursor = parent;
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(*succ);
                }
            }
        }
        None
    }

    fn format_incoming_value(op: &PcodeOp) -> String {
        format!(
            "seq={} op={:?} rhs={}",
            op.seq_num,
            op.opcode,
            Self::format_copy_overwrite_inputs(&op.inputs)
        )
    }

    fn path_crosses_switch_boundary(&self, start_idx: usize, target_idx: usize) -> Option<bool> {
        let path = self.shortest_forward_path(start_idx, target_idx)?;
        Some(path.into_iter().any(|idx| {
            self.pcode
                .blocks
                .get(idx)
                .and_then(|block| self.block_terminator_index(block))
                .and_then(|term_idx| self.pcode.blocks.get(idx)?.ops.get(term_idx))
                .is_some_and(|term_op| term_op.opcode == PcodeOpcode::BranchInd)
        }))
    }

    pub(super) fn copy_overwrite_restart_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_COPY_OVERWRITE_RESTART"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    pub(super) fn predicate_refresh_restart_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_PREDICATE_REFRESH_RESTART"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    pub(super) fn explicit_merge_binding_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_EXPLICIT_MERGE_BINDING"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    pub(super) fn can_restart_def_window_at_copy_overwrite(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<CopyOverwriteRestartProof> {
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
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

    pub(super) fn can_restart_def_window_at_predicate_refresh(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<PredicateOverwriteRefreshProof> {
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
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

    pub(super) fn copy_overwrite_rhs_is_pure_restart_candidate(
        redef: &CrossBlockRedefinitionDetail,
    ) -> bool {
        matches!(redef.redef_rhs_kind, SameBlockOverwriteRhsKind::CopyLike)
            && matches!(
                redef.overwrite_shape,
                SameBlockOverwriteShapeKind::OverwriteAtCopy
            )
    }

    pub(super) fn predicate_refresh_rhs_is_restart_candidate(
        redef: &CrossBlockRedefinitionDetail,
    ) -> bool {
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

    pub(super) fn no_alias_hazard_between_redef_and_terminator(
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

    pub(super) fn first_output_use_site_outside_block(
        &self,
        current_block_addr: u64,
        output: &Varnode,
    ) -> Option<(u64, usize, u32)> {
        let key = VarnodeKey::from(output);
        self.pcode
            .blocks
            .iter()
            .filter(|block| block.start_address != current_block_addr)
            .find_map(|block| {
                block
                    .ops
                    .iter()
                    .enumerate()
                    .find(|(_, candidate)| {
                        candidate
                            .inputs
                            .iter()
                            .any(|input| VarnodeKey::from(input) == key)
                    })
                    .map(|(idx, op)| (block.start_address, idx, op.seq_num))
            })
    }

    fn classify_missing_merge_binding_relation(
        &self,
        def_block_idx: usize,
        merge_block_idx: usize,
        consumer_op: &PcodeOp,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        predecessor_count: usize,
        has_existing_binding: bool,
    ) -> MissingMergeBindingRelation {
        if has_existing_binding {
            return MissingMergeBindingRelation::RepresentativeOnlyMissing;
        }
        if consumer_op.opcode == PcodeOpcode::MultiEqual {
            return MissingMergeBindingRelation::PhiLikeMergeMissing;
        }
        if matches!(
            consumer_kind,
            DisallowedSingleConsumerConsumerKind::Predicate
                | DisallowedSingleConsumerConsumerKind::BranchCondition
        ) {
            return MissingMergeBindingRelation::PredicateMergeMissing;
        }
        if predecessor_count > 1 {
            if self.block_can_reach(merge_block_idx, def_block_idx, merge_block_idx) {
                return MissingMergeBindingRelation::LoopHeaderMergeMissing;
            }
            return MissingMergeBindingRelation::JoinMergeMissing;
        }
        if self.block_can_reach(merge_block_idx, def_block_idx, merge_block_idx) {
            return MissingMergeBindingRelation::BackedgeMergeMissing;
        }
        MissingMergeBindingRelation::UnknownMissingMerge
    }

    fn classify_unknown_missing_merge_attribution_reason(
        &self,
        block_addr: u64,
        merge_block: u64,
        function_entry_block: u64,
        merge_block_is_entry: bool,
        predecessor_count: usize,
        successor_count: usize,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
    ) -> UnknownMissingMergeAttributionReason {
        if merge_block_is_entry && merge_block == block_addr {
            return UnknownMissingMergeAttributionReason::SelfMergeAtFunctionEntry;
        }
        if merge_block_is_entry && predecessor_count == 0 && successor_count > 1 {
            return UnknownMissingMergeAttributionReason::SyntheticRootBlock;
        }
        if merge_block_is_entry && merge_block == function_entry_block {
            return UnknownMissingMergeAttributionReason::EntryBlockAttribution;
        }
        if predecessor_count == 0 {
            return UnknownMissingMergeAttributionReason::MissingCfgPredecessors;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::StoreValue {
            return UnknownMissingMergeAttributionReason::StoreValueRepresentative;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::OtherData {
            return UnknownMissingMergeAttributionReason::OtherDataRepresentative;
        }
        UnknownMissingMergeAttributionReason::UnknownAttribution
    }

    pub(super) fn describe_missing_merge_binding_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        _op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<MissingMergeBindingProof> {
        let (merge_block_addr, consumer_op_idx, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let merge_block_idx = self.address_to_index.get(&merge_block_addr).copied()?;
        let merge_block = self.pcode.blocks.get(merge_block_idx)?;
        let consumer_op = merge_block.ops.get(consumer_op_idx)?;
        let key = VarnodeKey::from(output);
        let matched_inputs = consumer_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == key).then_some(idx))
            .collect::<Vec<_>>();
        let consumer_kind =
            Self::classify_disallowed_single_consumer_kind(consumer_op, &matched_inputs);
        let predecessor_count = self.predecessors.get(merge_block_idx).map_or(0, Vec::len);
        let incoming_value_count = if consumer_op.opcode == PcodeOpcode::MultiEqual {
            consumer_op.inputs.len()
        } else {
            predecessor_count.max(1)
        };
        let has_existing_binding = merge_block.ops[..consumer_op_idx]
            .iter()
            .any(|candidate| candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()));
        let relation = self.classify_missing_merge_binding_relation(
            def_block_idx,
            merge_block_idx,
            consumer_op,
            consumer_kind,
            predecessor_count,
            has_existing_binding,
        );
        Some(MissingMergeBindingProof {
            merge_block: merge_block_addr,
            predecessor_count,
            incoming_value_count,
            has_existing_binding,
            consumer_kind,
            rhs_kind: Self::classify_disallowed_single_consumer_rhs_kind(rhs),
            relation,
        })
    }

    fn classify_merge_binding_candidate_incoming_kind(
        op: &PcodeOp,
    ) -> MergeBindingCandidateIncomingKind {
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => MergeBindingCandidateIncomingKind::VarOrConst,
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow => MergeBindingCandidateIncomingKind::Predicate,
            PcodeOpcode::Load => MergeBindingCandidateIncomingKind::LoadLike,
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                MergeBindingCandidateIncomingKind::CallLike
            }
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
            | PcodeOpcode::IntNegate
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::PopCount => MergeBindingCandidateIncomingKind::Arithmetic,
            _ => MergeBindingCandidateIncomingKind::Unknown,
        }
    }

    fn collect_join_incoming_values(
        &self,
        merge_block_idx: usize,
        output: &Varnode,
    ) -> Option<(
        Vec<u64>,
        Vec<String>,
        usize,
        bool,
        bool,
        usize,
        BTreeSet<MergeBindingCandidateIncomingKind>,
    )> {
        let predecessor_idxs = self.predecessors.get(merge_block_idx)?.clone();
        let predecessor_blocks = predecessor_idxs
            .iter()
            .filter_map(|idx| self.pcode.blocks.get(*idx).map(|block| block.start_address))
            .collect::<Vec<_>>();
        let mut incoming_values = Vec::new();
        let mut defined_values = Vec::new();
        let mut incoming_value_kinds = BTreeSet::new();

        for pred_idx in predecessor_idxs {
            let pred_block = self.pcode.blocks.get(pred_idx)?;
            let pred_value = Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                .map(|(_, op)| {
                    incoming_value_kinds
                        .insert(Self::classify_merge_binding_candidate_incoming_kind(op));
                    Self::format_incoming_value(op)
                });
            if let Some(value) = pred_value.clone() {
                defined_values.push(value);
            }
            incoming_values.push(format!(
                "pred=0x{:x}:{}",
                pred_block.start_address,
                pred_value.unwrap_or_else(|| "none".to_string())
            ));
        }

        let incoming_value_count = defined_values.len();
        let distinct_values = defined_values.iter().cloned().collect::<HashSet<_>>();
        let values_same_across_preds = !defined_values.is_empty() && distinct_values.len() == 1;
        let has_conflicting_incoming = distinct_values.len() > 1;
        Some((
            predecessor_blocks,
            incoming_values,
            incoming_value_count,
            values_same_across_preds,
            has_conflicting_incoming,
            distinct_values.len(),
            incoming_value_kinds,
        ))
    }

    fn classify_join_merge_missing_reason(
        values_same_across_preds: bool,
        has_missing_incoming: bool,
        has_conflicting_incoming: bool,
        incoming_value_count: usize,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
    ) -> JoinMergeMissingReason {
        if values_same_across_preds && !has_missing_incoming {
            return JoinMergeMissingReason::AllIncomingSame;
        }
        if has_missing_incoming {
            return JoinMergeMissingReason::MissingIncomingForSomePred;
        }
        if has_conflicting_incoming {
            return JoinMergeMissingReason::ConflictingIncomingValues;
        }
        if incoming_value_count == 1 {
            return JoinMergeMissingReason::SinglePredValueOnly;
        }
        match consumer_kind {
            DisallowedSingleConsumerConsumerKind::StoreValue => {
                JoinMergeMissingReason::StoreValueMerge
            }
            DisallowedSingleConsumerConsumerKind::OtherData => {
                JoinMergeMissingReason::OtherDataMerge
            }
            DisallowedSingleConsumerConsumerKind::Predicate
            | DisallowedSingleConsumerConsumerKind::BranchCondition => {
                JoinMergeMissingReason::PredicateMerge
            }
            _ => JoinMergeMissingReason::UnknownJoinMerge,
        }
    }

    pub(super) fn describe_join_merge_missing_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<JoinMergeMissingProof> {
        let proof = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)?;
        if proof.relation != MissingMergeBindingRelation::JoinMergeMissing {
            return None;
        }
        let merge_block_idx = self.address_to_index.get(&proof.merge_block).copied()?;
        let (
            predecessor_blocks,
            incoming_values,
            incoming_value_count,
            values_same_across_preds,
            has_conflicting_incoming,
            _distinct_incoming_value_count,
            _incoming_value_kinds,
        ) = self.collect_join_incoming_values(merge_block_idx, output)?;
        let has_missing_incoming = incoming_value_count < predecessor_blocks.len();
        let reason = Self::classify_join_merge_missing_reason(
            values_same_across_preds,
            has_missing_incoming,
            has_conflicting_incoming,
            incoming_value_count,
            proof.consumer_kind,
        );
        Some(JoinMergeMissingProof {
            event_block: block.start_address,
            merge_block: proof.merge_block,
            predecessor_blocks,
            incoming_value_count,
            incoming_values,
            values_same_across_preds,
            has_missing_incoming,
            has_conflicting_incoming,
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            reason,
        })
    }

    fn classify_merge_binding_candidate_result(
        missing_incoming_count: usize,
        conflicting_incoming_count: usize,
        incoming_value_kinds: &BTreeSet<MergeBindingCandidateIncomingKind>,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
    ) -> MergeBindingCandidateResult {
        if missing_incoming_count > 0 {
            return MergeBindingCandidateResult::MissingIncomingSemanticsRequired;
        }
        if conflicting_incoming_count == 0 {
            return MergeBindingCandidateResult::InsufficientConflictingIncoming;
        }
        let incoming_kinds_safe = incoming_value_kinds.iter().all(|kind| {
            matches!(
                kind,
                MergeBindingCandidateIncomingKind::VarOrConst
                    | MergeBindingCandidateIncomingKind::Predicate
                    | MergeBindingCandidateIncomingKind::Arithmetic
            )
        });
        let consumer_safe = matches!(
            consumer_kind,
            DisallowedSingleConsumerConsumerKind::OtherData
                | DisallowedSingleConsumerConsumerKind::Predicate
                | DisallowedSingleConsumerConsumerKind::StoreValue
        );
        if incoming_kinds_safe && consumer_safe {
            return MergeBindingCandidateResult::PhiLikeBindingCandidate;
        }
        MergeBindingCandidateResult::IncomingKindsUnsafe
    }

    pub(super) fn describe_merge_binding_candidate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<MergeBindingCandidateProof> {
        let proof = self.describe_join_merge_missing_proof(block, op_idx, output, rhs)?;
        let merge_block_idx = self.address_to_index.get(&proof.merge_block).copied()?;
        let (
            predecessor_blocks,
            _incoming_values,
            incoming_value_count,
            _values_same_across_preds,
            _has_conflicting_incoming,
            distinct_incoming_value_count,
            incoming_value_kinds,
        ) = self.collect_join_incoming_values(merge_block_idx, output)?;
        let missing_incoming_count = predecessor_blocks
            .len()
            .saturating_sub(incoming_value_count);
        let conflicting_incoming_count = distinct_incoming_value_count.saturating_sub(1);
        let result = Self::classify_merge_binding_candidate_result(
            missing_incoming_count,
            conflicting_incoming_count,
            &incoming_value_kinds,
            proof.consumer_kind,
        );
        Some(MergeBindingCandidateProof {
            merge_block: proof.merge_block,
            predecessor_count: predecessor_blocks.len(),
            missing_incoming_count,
            conflicting_incoming_count,
            incoming_value_kinds: incoming_value_kinds.into_iter().collect(),
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            can_synthesize_phi_like_binding: matches!(
                result,
                MergeBindingCandidateResult::PhiLikeBindingCandidate
            ),
            result,
        })
    }

    pub(super) fn describe_explicit_merge_binding_trial(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Result<MergeBindingCandidateProof, ExplicitMergeBindingTrialReason> {
        let Some(missing_proof) =
            self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)
        else {
            return Err(ExplicitMergeBindingTrialReason::RejectedNotJoinMerge);
        };
        if missing_proof.relation != MissingMergeBindingRelation::JoinMergeMissing {
            return Err(ExplicitMergeBindingTrialReason::RejectedNotJoinMerge);
        }
        let Some(proof) = self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)
        else {
            return Err(ExplicitMergeBindingTrialReason::RejectedNotJoinMerge);
        };
        if proof.predecessor_count != 2 {
            return Err(ExplicitMergeBindingTrialReason::RejectedNonBinaryPreds);
        }
        if proof.missing_incoming_count > 0 {
            return Err(ExplicitMergeBindingTrialReason::RejectedMissingIncoming);
        }
        if proof.conflicting_incoming_count != 1 {
            return Err(ExplicitMergeBindingTrialReason::RejectedMultipleConflicts);
        }
        if proof.consumer_kind != DisallowedSingleConsumerConsumerKind::OtherData {
            return Err(ExplicitMergeBindingTrialReason::RejectedConsumerKind);
        }
        if !proof.incoming_value_kinds.iter().all(|kind| {
            matches!(
                kind,
                MergeBindingCandidateIncomingKind::VarOrConst
                    | MergeBindingCandidateIncomingKind::Arithmetic
            )
        }) {
            return Err(ExplicitMergeBindingTrialReason::RejectedUnsafeIncomingKind);
        }
        if !proof.can_synthesize_phi_like_binding {
            return Err(ExplicitMergeBindingTrialReason::RejectedUnsafeIncomingKind);
        }
        Ok(proof)
    }

    pub(super) fn synthesize_explicit_merge_bindings_for_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        if !Self::explicit_merge_binding_enabled() {
            return Ok(Vec::new());
        }
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
            return Ok(Vec::new());
        };
        let Some(predecessor_idxs) = self.predecessors.get(block_idx).cloned() else {
            return Ok(Vec::new());
        };

        struct PendingMergeBinding {
            output: Varnode,
            predecessor_blocks: Vec<u64>,
            incoming_value_kinds: Vec<MergeBindingCandidateIncomingKind>,
            incoming_values: Vec<String>,
            rhs_kind: DisallowedSingleConsumerRhsKind,
            incoming_by_pred: HashMap<u64, HirExpr>,
        }

        let mut pending: HashMap<VarnodeKey, PendingMergeBinding> = HashMap::new();
        for pred_idx in predecessor_idxs {
            let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
                continue;
            };
            for (op_idx, op) in pred_block.ops.iter().enumerate() {
                let Some(output) = &op.output else {
                    continue;
                };
                let Some(rhs) =
                    self.try_lower_materialized_output_rhs(pred_block.start_address, op)?
                else {
                    continue;
                };
                let Ok(proof) =
                    self.describe_explicit_merge_binding_trial(pred_block, op_idx, output, &rhs)
                else {
                    continue;
                };
                if proof.merge_block != block.start_address {
                    continue;
                }
                let Some(join_proof) =
                    self.describe_join_merge_missing_proof(pred_block, op_idx, output, &rhs)
                else {
                    continue;
                };
                let key = VarnodeKey::from(output);
                let entry = pending.entry(key).or_insert_with(|| PendingMergeBinding {
                    output: output.clone(),
                    predecessor_blocks: join_proof.predecessor_blocks.clone(),
                    incoming_value_kinds: proof.incoming_value_kinds.clone(),
                    incoming_values: join_proof.incoming_values.clone(),
                    rhs_kind: proof.rhs_kind,
                    incoming_by_pred: HashMap::new(),
                });
                entry
                    .incoming_by_pred
                    .insert(pred_block.start_address, rhs.clone());
            }
        }

        let mut entries = pending.into_iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| (key.space_id, key.offset, key.size));

        let mut stmts = Vec::new();
        for (_key, pending) in entries {
            if pending.predecessor_blocks.len() != 2
                || !pending
                    .predecessor_blocks
                    .iter()
                    .all(|pred| pending.incoming_by_pred.contains_key(pred))
            {
                continue;
            }
            let args = pending
                .predecessor_blocks
                .iter()
                .filter_map(|pred| pending.incoming_by_pred.get(pred).cloned())
                .collect::<Vec<_>>();
            if args.len() != 2 {
                continue;
            }
            let binding = self.ensure_explicit_merge_binding_for_block(block_idx, &pending.output);
            self.trace_explicit_merge_binding_trial(
                block.start_address,
                &pending.output,
                &pending.predecessor_blocks,
                &pending.incoming_values,
                &pending.incoming_value_kinds,
                pending.rhs_kind,
                &binding.name,
                true,
                ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
            );
            stmts.push(HirStmt::Assign {
                lhs: HirLValue::Var(binding.name),
                rhs: HirExpr::Call {
                    target: "__fission_merge2".to_string(),
                    args,
                    ty: type_from_size(pending.output.size, false),
                },
            });
        }

        Ok(stmts)
    }

    fn classify_missing_incoming_pred_kind(
        pred_is_entry: bool,
        pred_reachable_from_entry: bool,
        merge_reaches_pred: bool,
        pred_has_prior_definition: bool,
        prior_def_dominates_pred: bool,
    ) -> MissingIncomingPredKind {
        if pred_is_entry {
            return MissingIncomingPredKind::MissingBecauseEntryDefault;
        }
        if !pred_reachable_from_entry {
            return MissingIncomingPredKind::MissingBecauseDeadPred;
        }
        if merge_reaches_pred {
            return MissingIncomingPredKind::MissingBecauseLoopBackedge;
        }
        if pred_has_prior_definition && prior_def_dominates_pred {
            return MissingIncomingPredKind::MissingBecausePriorDefDominates;
        }
        if pred_has_prior_definition {
            return MissingIncomingPredKind::MissingBecausePathSensitive;
        }
        MissingIncomingPredKind::MissingBecauseNoPriorDef
    }

    fn classify_dominating_prior_def_proof_result(
        prior_def_dominates_merge: bool,
        redefined_between_prior_and_merge: bool,
        redefined_on_pred_path: bool,
        crosses_call_or_store: bool,
    ) -> DominatingPriorDefProofResult {
        if !prior_def_dominates_merge {
            if redefined_on_pred_path || redefined_between_prior_and_merge {
                return DominatingPriorDefProofResult::PriorDefPathSensitive;
            }
            return DominatingPriorDefProofResult::PriorDefDoesNotDominateMerge;
        }
        if redefined_on_pred_path || redefined_between_prior_and_merge {
            return DominatingPriorDefProofResult::PriorDefRedefinedBeforeMerge;
        }
        if crosses_call_or_store {
            return DominatingPriorDefProofResult::PriorDefCrossesCallOrStore;
        }
        DominatingPriorDefProofResult::PriorDefStableToMerge
    }

    fn classify_missing_no_prior_def_reason(
        pred_is_entry: bool,
        pred_is_dead: bool,
        output: &Varnode,
    ) -> (MissingNoPriorDefReason, String) {
        if pred_is_dead {
            return (
                MissingNoPriorDefReason::DeadPredNoDef,
                "dead-pred".to_string(),
            );
        }
        if pred_is_entry {
            return (
                MissingNoPriorDefReason::EntryDefaultCandidate,
                "entry-default".to_string(),
            );
        }
        if output.space_id == REGISTER_SPACE_ID {
            return (
                MissingNoPriorDefReason::RegisterDefault,
                "register-default".to_string(),
            );
        }
        if output.space_id == UNIQUE_SPACE_ID && !output.is_constant {
            return (
                MissingNoPriorDefReason::TempOnlyNoDef,
                "temp-only".to_string(),
            );
        }
        if output.is_constant {
            return (
                MissingNoPriorDefReason::UndefinedIncoming,
                "const-undef".to_string(),
            );
        }
        (MissingNoPriorDefReason::TrueNoPriorDef, "none".to_string())
    }

    fn classify_temp_only_representative_reason(
        root_attributed: bool,
        dead_pred: bool,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        source_event: &str,
    ) -> TempOnlyRepresentativeReason {
        if dead_pred {
            return TempOnlyRepresentativeReason::DeadTempRepresentative;
        }
        if root_attributed {
            return TempOnlyRepresentativeReason::RootAttributedTemp;
        }
        if source_event == "TempOnlyNoDef" {
            return TempOnlyRepresentativeReason::TempRepresentativeResidue;
        }
        match consumer_kind {
            DisallowedSingleConsumerConsumerKind::StoreValue => {
                TempOnlyRepresentativeReason::StoreValueTemp
            }
            DisallowedSingleConsumerConsumerKind::OtherData => {
                TempOnlyRepresentativeReason::OtherDataTemp
            }
            _ => TempOnlyRepresentativeReason::MergeCrossingTemp,
        }
    }

    pub(super) fn describe_missing_incoming_pred_proofs(
        &self,
        event_block: u64,
        merge_block: u64,
        output: &Varnode,
    ) -> Vec<MissingIncomingPredProof> {
        let Some(merge_block_idx) = self.address_to_index.get(&merge_block).copied() else {
            return Vec::new();
        };
        let Some(entry_block_addr) = self.pcode.blocks.first().map(|block| block.start_address)
        else {
            return Vec::new();
        };
        let Some(entry_block_idx) = self.address_to_index.get(&entry_block_addr).copied() else {
            return Vec::new();
        };
        let Some(predecessor_idxs) = self.predecessors.get(merge_block_idx) else {
            return Vec::new();
        };

        predecessor_idxs
            .iter()
            .filter_map(|pred_idx| {
                let pred_block = self.pcode.blocks.get(*pred_idx)?;
                let pred_has_definition =
                    Self::first_output_redefinition_in_block_from(pred_block, 0, output).is_some();
                if pred_has_definition {
                    return None;
                }

                let best_prior_def = self.best_prior_definition_for_missing_pred(*pred_idx, output);

                let pred_reaches_merge =
                    self.block_can_reach(*pred_idx, merge_block_idx, merge_block_idx);
                let pred_reachable_from_entry = self
                    .shortest_forward_distance(entry_block_idx, *pred_idx)
                    .is_some();
                let pred_has_prior_definition = best_prior_def.is_some();
                let prior_def_dominates_pred = best_prior_def
                    .as_ref()
                    .map(|candidate| candidate.4)
                    .unwrap_or(false);
                let incoming_kind = Self::classify_missing_incoming_pred_kind(
                    pred_block.start_address == entry_block_addr,
                    pred_reachable_from_entry,
                    self.block_can_reach(merge_block_idx, *pred_idx, merge_block_idx),
                    pred_has_prior_definition,
                    prior_def_dominates_pred,
                );

                Some(MissingIncomingPredProof {
                    event_block,
                    merge_block,
                    pred_block: pred_block.start_address,
                    pred_reaches_merge,
                    pred_has_definition,
                    pred_has_prior_definition,
                    prior_def_block: best_prior_def.as_ref().map(|candidate| candidate.1),
                    prior_def_op_seq: best_prior_def.as_ref().map(|candidate| candidate.3),
                    incoming_kind,
                })
            })
            .collect()
    }

    pub(super) fn describe_dominating_prior_def_incoming_proof(
        &self,
        merge_block: u64,
        pred_block: u64,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
    ) -> Option<DominatingPriorDefIncomingProof> {
        let merge_block_idx = self.address_to_index.get(&merge_block).copied()?;
        let pred_block_idx = self.address_to_index.get(&pred_block).copied()?;
        let best_prior_def = self.best_prior_definition_for_missing_pred(pred_block_idx, output)?;
        if !best_prior_def.4 {
            return None;
        }
        let prior_def_dominates_merge = self.dom_tree.dominates(best_prior_def.0, merge_block_idx);
        let redefined_between_prior_and_merge = self.path_slice_has_output_redefinition(
            best_prior_def.0,
            Some(best_prior_def.2),
            merge_block_idx,
            output,
        );
        let redefined_on_pred_path =
            self.path_slice_has_output_redefinition(pred_block_idx, None, merge_block_idx, output);
        let crosses_call_or_store = self.path_slice_crosses_call_or_store(
            best_prior_def.0,
            Some(best_prior_def.2),
            merge_block_idx,
        );
        let proof_result = Self::classify_dominating_prior_def_proof_result(
            prior_def_dominates_merge,
            redefined_between_prior_and_merge,
            redefined_on_pred_path,
            crosses_call_or_store,
        );
        Some(DominatingPriorDefIncomingProof {
            merge_block,
            pred_block,
            prior_def_block: best_prior_def.1,
            prior_def_op_seq: best_prior_def.3,
            prior_def_rhs: best_prior_def.6,
            prior_def_dominates_pred: best_prior_def.4,
            prior_def_dominates_merge,
            redefined_between_prior_and_merge,
            redefined_on_pred_path,
            consumer_kind,
            rhs_kind,
            proof_result,
        })
    }

    pub(super) fn describe_missing_no_prior_def_proof(
        &self,
        merge_block: u64,
        pred_block: u64,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
    ) -> Option<MissingNoPriorDefProof> {
        let merge_block_idx = self.address_to_index.get(&merge_block).copied()?;
        let pred_block_idx = self.address_to_index.get(&pred_block).copied()?;
        let entry_block_addr = self.pcode.blocks.first()?.start_address;
        let entry_block_idx = self.address_to_index.get(&entry_block_addr).copied()?;
        let pred_block_ref = self.pcode.blocks.get(pred_block_idx)?;
        if Self::first_output_redefinition_in_block_from(pred_block_ref, 0, output).is_some() {
            return None;
        }
        if self
            .best_prior_definition_for_missing_pred(pred_block_idx, output)
            .is_some()
        {
            return None;
        }
        let pred_reaches_merge =
            self.block_can_reach(pred_block_idx, merge_block_idx, merge_block_idx);
        let pred_is_entry = pred_block == entry_block_addr;
        let pred_is_dead = self
            .shortest_forward_distance(entry_block_idx, pred_block_idx)
            .is_none();
        let (reason, default_candidate) =
            Self::classify_missing_no_prior_def_reason(pred_is_entry, pred_is_dead, output);
        Some(MissingNoPriorDefProof {
            merge_block,
            pred_block,
            pred_reaches_merge,
            pred_is_entry,
            pred_is_dead,
            output_space: output.space_id,
            output_size: output.size,
            consumer_kind,
            rhs_kind,
            default_candidate,
            reason,
        })
    }

    pub(super) fn describe_temp_only_representative_proof(
        &self,
        merge_block: u64,
        pred_block: Option<u64>,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
        source_event: &str,
        root_attributed: bool,
        dead_pred: bool,
    ) -> Option<TempOnlyRepresentativeProof> {
        if output.space_id != UNIQUE_SPACE_ID || output.is_constant {
            return None;
        }
        let reason = Self::classify_temp_only_representative_reason(
            root_attributed,
            dead_pred,
            consumer_kind,
            source_event,
        );
        Some(TempOnlyRepresentativeProof {
            merge_block,
            pred_block,
            consumer_kind,
            rhs_kind,
            defining_event: format!(
                "space:{} off:0x{:x} size:{}",
                output.space_id, output.offset, output.size
            ),
            materialization_event: source_event.to_string(),
            has_real_storage: false,
            has_later_use: true,
            crosses_merge: true,
            root_attributed,
            reason,
        })
    }

    pub(super) fn describe_temp_only_representative_site_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<TempOnlyRepresentativeProof> {
        let missing = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)?;
        if missing.relation == MissingMergeBindingRelation::RepresentativeOnlyMissing {
            return self.describe_temp_only_representative_proof(
                missing.merge_block,
                None,
                output,
                missing.consumer_kind,
                missing.rhs_kind,
                "RepresentativeOnlyMissing",
                false,
                false,
            );
        }
        if let Some(unknown) =
            self.describe_unknown_missing_merge_attribution(block, op_idx, output, rhs)
        {
            let source_event = match unknown.reason {
                UnknownMissingMergeAttributionReason::SyntheticRootBlock => {
                    Some(("SyntheticRootBlock", true))
                }
                UnknownMissingMergeAttributionReason::OtherDataRepresentative => {
                    Some(("OtherDataRepresentative", false))
                }
                _ => None,
            };
            if let Some((source_event, root_attributed)) = source_event {
                return self.describe_temp_only_representative_proof(
                    unknown.merge_block,
                    None,
                    output,
                    unknown.consumer_kind,
                    unknown.rhs_kind,
                    source_event,
                    root_attributed,
                    false,
                );
            }
        }
        for incoming in self.describe_missing_incoming_pred_proofs(
            block.start_address,
            missing.merge_block,
            output,
        ) {
            if matches!(
                incoming.incoming_kind,
                MissingIncomingPredKind::MissingBecauseNoPriorDef
                    | MissingIncomingPredKind::MissingBecauseDeadPred
                    | MissingIncomingPredKind::MissingBecauseEntryDefault
            ) {
                if let Some(no_prior) = self.describe_missing_no_prior_def_proof(
                    incoming.merge_block,
                    incoming.pred_block,
                    output,
                    missing.consumer_kind,
                    missing.rhs_kind,
                ) {
                    let source_event = match no_prior.reason {
                        MissingNoPriorDefReason::TempOnlyNoDef => Some(("TempOnlyNoDef", false)),
                        MissingNoPriorDefReason::DeadPredNoDef => Some(("DeadPredNoDef", false)),
                        _ => None,
                    };
                    if let Some((source_event, root_attributed)) = source_event {
                        return self.describe_temp_only_representative_proof(
                            no_prior.merge_block,
                            Some(no_prior.pred_block),
                            output,
                            no_prior.consumer_kind,
                            no_prior.rhs_kind,
                            source_event,
                            root_attributed,
                            no_prior.reason == MissingNoPriorDefReason::DeadPredNoDef,
                        );
                    }
                }
            }
        }
        None
    }

    pub(super) fn describe_unknown_missing_merge_attribution(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<UnknownMissingMergeAttributionProof> {
        let proof = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)?;
        if proof.relation != MissingMergeBindingRelation::UnknownMissingMerge {
            return None;
        }
        let function_entry_block = self.pcode.blocks.first()?.start_address;
        let merge_block_idx = self.address_to_index.get(&proof.merge_block).copied();
        let merge_block_is_entry = proof.merge_block == function_entry_block;
        let successor_count = merge_block_idx
            .and_then(|idx| self.successors.get(idx))
            .map_or(0, Vec::len);
        let reason = self.classify_unknown_missing_merge_attribution_reason(
            block.start_address,
            proof.merge_block,
            function_entry_block,
            merge_block_is_entry,
            proof.predecessor_count,
            successor_count,
            proof.consumer_kind,
        );
        Some(UnknownMissingMergeAttributionProof {
            merge_block: proof.merge_block,
            function_entry_block,
            merge_block_is_entry,
            predecessor_count: proof.predecessor_count,
            successor_count,
            incoming_value_count: proof.incoming_value_count,
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            reason,
        })
    }

    fn classify_synthetic_root_merge_reason(
        selected_is_entry: bool,
        nearest_join_block: Option<usize>,
        nearest_postdom_join: Option<usize>,
        has_existing_binding: bool,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
    ) -> SyntheticRootMergeAttributionReason {
        if has_existing_binding {
            return SyntheticRootMergeAttributionReason::RootRepresentativeOnly;
        }
        if nearest_join_block.is_none() && nearest_postdom_join.is_none() {
            return SyntheticRootMergeAttributionReason::NoNearestJoinFound;
        }
        if nearest_join_block.is_some() || nearest_postdom_join.is_some() {
            return SyntheticRootMergeAttributionReason::ForwardJoinExistsButNotSelected;
        }
        if selected_is_entry {
            return SyntheticRootMergeAttributionReason::EntryBlockAsMergeFallback;
        }
        match consumer_kind {
            DisallowedSingleConsumerConsumerKind::StoreValue => {
                SyntheticRootMergeAttributionReason::StoreValueAtRoot
            }
            DisallowedSingleConsumerConsumerKind::OtherData => {
                SyntheticRootMergeAttributionReason::OtherDataAtRoot
            }
            _ => SyntheticRootMergeAttributionReason::UnknownRootAttribution,
        }
    }

    pub(super) fn describe_synthetic_root_merge_attribution(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<SyntheticRootMergeAttributionProof> {
        let unknown =
            self.describe_unknown_missing_merge_attribution(block, op_idx, output, rhs)?;
        if unknown.reason != UnknownMissingMergeAttributionReason::SyntheticRootBlock {
            return None;
        }
        let event_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let entry_block_idx = self
            .address_to_index
            .get(&unknown.function_entry_block)
            .copied()?;
        let selected_merge_idx = self.address_to_index.get(&unknown.merge_block).copied()?;
        let nearest_join = self.nearest_forward_join_block(event_block_idx);
        let nearest_postdom_join = self.nearest_postdom_join_block(event_block_idx);
        let missing = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)?;
        let reason = Self::classify_synthetic_root_merge_reason(
            unknown.merge_block_is_entry,
            nearest_join.map(|(idx, _)| idx),
            nearest_postdom_join.map(|(idx, _)| idx),
            missing.has_existing_binding,
            unknown.consumer_kind,
        );
        Some(SyntheticRootMergeAttributionProof {
            event_block: block.start_address,
            entry_block: unknown.function_entry_block,
            selected_merge_block: unknown.merge_block,
            selected_is_entry: unknown.merge_block_is_entry,
            event_block_is_entry: block.start_address == unknown.function_entry_block,
            event_block_dominates: self.dom_tree.dominates(event_block_idx, selected_merge_idx),
            nearest_join_block: nearest_join
                .map(|(idx, _)| self.pcode.blocks.get(idx).map(|b| b.start_address))
                .flatten(),
            nearest_join_distance: nearest_join.map(|(_, distance)| distance),
            nearest_postdom_join: nearest_postdom_join
                .map(|(idx, _)| self.pcode.blocks.get(idx).map(|b| b.start_address))
                .flatten(),
            postdom_distance: nearest_postdom_join.map(|(_, distance)| distance),
            block_successor_count: self.successors.get(event_block_idx).map_or(0, Vec::len),
            entry_successor_count: self.successors.get(entry_block_idx).map_or(0, Vec::len),
            consumer_kind: unknown.consumer_kind,
            rhs_kind: unknown.rhs_kind,
            reason,
        })
    }

    fn classify_forward_join_not_selected_rejected_reason(
        &self,
        event_block_idx: usize,
        forward_join_idx: usize,
        event_reaches_forward_join: bool,
        forward_join_postdominates_event: Option<bool>,
        forward_join_predecessor_count: usize,
        crosses_switch_boundary: bool,
    ) -> ForwardJoinNotSelectedRejectedReason {
        if !event_reaches_forward_join {
            return ForwardJoinNotSelectedRejectedReason::JoinNotReachableFromEvent;
        }
        if crosses_switch_boundary {
            return ForwardJoinNotSelectedRejectedReason::JoinCrossesSwitchBoundary;
        }
        if self.block_can_reach(forward_join_idx, event_block_idx, forward_join_idx) {
            return ForwardJoinNotSelectedRejectedReason::JoinCrossesLoopBoundary;
        }
        let Some(forward_join_postdominates_event) = forward_join_postdominates_event else {
            return ForwardJoinNotSelectedRejectedReason::JoinDominanceUnknown;
        };
        if !forward_join_postdominates_event {
            return ForwardJoinNotSelectedRejectedReason::JoinDoesNotPostdominate;
        }
        if forward_join_predecessor_count > 2 {
            return ForwardJoinNotSelectedRejectedReason::JoinHasMultipleAmbiguousPreds;
        }
        ForwardJoinNotSelectedRejectedReason::JoinRejectedByCurrentHeuristic
    }

    pub(super) fn describe_forward_join_not_selected_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<ForwardJoinNotSelectedProof> {
        let synthetic =
            self.describe_synthetic_root_merge_attribution(block, op_idx, output, rhs)?;
        if synthetic.reason != SyntheticRootMergeAttributionReason::ForwardJoinExistsButNotSelected
        {
            return None;
        }
        let event_block_idx = self.address_to_index.get(&synthetic.event_block).copied()?;
        let forward_join = self
            .nearest_postdom_join_block(event_block_idx)
            .or_else(|| self.nearest_forward_join_block(event_block_idx))?;
        let forward_join_idx = forward_join.0;
        let forward_join_block = self.pcode.blocks.get(forward_join_idx)?;
        let event_reaches_forward_join = self
            .shortest_forward_distance(event_block_idx, forward_join_idx)
            .is_some();
        let forward_join_postdominates_event = self
            .cfg_facts
            .postdominators()
            .postdominators()
            .get(&event_block_idx)
            .map(|set| set.contains(&forward_join_idx));
        let forward_join_predecessor_count =
            self.predecessors.get(forward_join_idx).map_or(0, Vec::len);
        let forward_join_successor_count =
            self.successors.get(forward_join_idx).map_or(0, Vec::len);
        let crosses_switch_boundary = self
            .path_crosses_switch_boundary(event_block_idx, forward_join_idx)
            .unwrap_or(false);
        let rejected_reason = self.classify_forward_join_not_selected_rejected_reason(
            event_block_idx,
            forward_join_idx,
            event_reaches_forward_join,
            forward_join_postdominates_event,
            forward_join_predecessor_count,
            crosses_switch_boundary,
        );
        Some(ForwardJoinNotSelectedProof {
            event_block: synthetic.event_block,
            selected_merge_block: synthetic.selected_merge_block,
            forward_join_block: forward_join_block.start_address,
            forward_join_distance: Some(forward_join.1),
            forward_join_predecessor_count,
            forward_join_successor_count,
            event_reaches_forward_join,
            forward_join_postdominates_event: forward_join_postdominates_event.unwrap_or(false),
            consumer_kind: synthetic.consumer_kind,
            rhs_kind: synthetic.rhs_kind,
            rejected_reason,
        })
    }

    pub(super) fn describe_ambiguous_join_pred_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<AmbiguousJoinPredProof> {
        let proof = self.describe_forward_join_not_selected_proof(block, op_idx, output, rhs)?;
        if proof.rejected_reason
            != ForwardJoinNotSelectedRejectedReason::JoinHasMultipleAmbiguousPreds
        {
            return None;
        }
        let event_block_idx = self.address_to_index.get(&proof.event_block).copied()?;
        let forward_join_idx = self
            .address_to_index
            .get(&proof.forward_join_block)
            .copied()?;
        let predecessor_idxs = self.predecessors.get(forward_join_idx)?.clone();
        let predecessor_blocks = predecessor_idxs
            .iter()
            .filter_map(|idx| self.pcode.blocks.get(*idx).map(|block| block.start_address))
            .collect::<Vec<_>>();
        let mut incoming_values = Vec::new();
        let mut event_pred_index = None;
        let mut event_pred_value = None;

        for (pred_list_idx, pred_idx) in predecessor_idxs.iter().enumerate() {
            let pred_block = self.pcode.blocks.get(*pred_idx)?;
            let pred_value = Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                .map(|(_, op)| Self::format_incoming_value(op));
            if event_pred_index.is_none()
                && self.block_can_reach(event_block_idx, *pred_idx, forward_join_idx)
            {
                event_pred_index = Some(pred_list_idx);
                event_pred_value = pred_value.clone();
            }
            incoming_values.push(format!(
                "pred=0x{:x}:{}",
                pred_block.start_address,
                pred_value.clone().unwrap_or_else(|| "none".to_string())
            ));
        }

        let defined_values = predecessor_idxs
            .iter()
            .filter_map(|pred_idx| {
                self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                    Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                        .map(|(_, op)| Self::format_incoming_value(op))
                })
            })
            .collect::<Vec<_>>();
        let incoming_value_count = defined_values.len();
        let distinct_values = defined_values.iter().cloned().collect::<HashSet<_>>();
        let values_same_across_preds = !defined_values.is_empty() && distinct_values.len() == 1;
        let has_missing_incoming = incoming_value_count < predecessor_idxs.len();
        let has_conflicting_incoming = distinct_values.len() > 1;
        let reason = Self::classify_ambiguous_join_pred_reason(
            values_same_across_preds,
            has_missing_incoming,
            has_conflicting_incoming,
            event_pred_index.is_some() && event_pred_value.is_some(),
            incoming_value_count,
            proof.consumer_kind,
        );

        Some(AmbiguousJoinPredProof {
            event_block: proof.event_block,
            forward_join_block: proof.forward_join_block,
            predecessor_blocks,
            incoming_value_count,
            incoming_values,
            event_pred_index,
            event_pred_value,
            values_same_across_preds,
            has_missing_incoming,
            has_conflicting_incoming,
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            reason,
        })
    }

    fn classify_ambiguous_join_pred_reason(
        values_same_across_preds: bool,
        has_missing_incoming: bool,
        has_conflicting_incoming: bool,
        event_pred_has_value: bool,
        incoming_value_count: usize,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
    ) -> AmbiguousJoinPredReason {
        if values_same_across_preds && !has_missing_incoming {
            return AmbiguousJoinPredReason::AllIncomingSame;
        }
        if has_missing_incoming {
            return AmbiguousJoinPredReason::MissingIncomingForSomePred;
        }
        if has_conflicting_incoming {
            return AmbiguousJoinPredReason::ConflictingIncomingValues;
        }
        if event_pred_has_value && incoming_value_count == 1 {
            return AmbiguousJoinPredReason::EventPredOnlyValue;
        }
        match consumer_kind {
            DisallowedSingleConsumerConsumerKind::StoreValue => {
                AmbiguousJoinPredReason::StoreValueAmbiguous
            }
            DisallowedSingleConsumerConsumerKind::OtherData => {
                AmbiguousJoinPredReason::OtherDataAmbiguous
            }
            _ => AmbiguousJoinPredReason::UnknownAmbiguousJoin,
        }
    }

    pub(super) fn classify_malformed_def_use_window_relation(
        def_op_idx: usize,
        terminator_idx: Option<usize>,
        first_same_block_consumer_idx: Option<usize>,
        first_cross_block_consumer: Option<(u64, usize, u32)>,
        block_index_present: bool,
        has_redefinition: bool,
    ) -> MalformedDefUseWindowRelation {
        if !block_index_present {
            return MalformedDefUseWindowRelation::BlockMismatch;
        }
        let Some(terminator_idx) = terminator_idx else {
            return MalformedDefUseWindowRelation::TerminatorMissing;
        };
        if def_op_idx > terminator_idx {
            return MalformedDefUseWindowRelation::DefAfterTerminator;
        }
        if let Some(consumer_idx) = first_same_block_consumer_idx {
            if consumer_idx < def_op_idx {
                return MalformedDefUseWindowRelation::ConsumerBeforeDef;
            }
            if consumer_idx > terminator_idx {
                return MalformedDefUseWindowRelation::ConsumerAfterTerminator;
            }
        }
        if first_cross_block_consumer.is_some() {
            return MalformedDefUseWindowRelation::ConsumerInDifferentBlock;
        }
        if has_redefinition {
            return MalformedDefUseWindowRelation::RedefinitionBeforeConsumer;
        }
        MalformedDefUseWindowRelation::UnknownWindow
    }

    pub(super) fn describe_malformed_def_use_window(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> MalformedDefUseWindowDetail {
        let rhs_kind = Self::classify_no_consumer_suppression_rhs_kind(rhs);
        let terminator_idx = self.block_terminator_index(block);
        let block_index_present = self.address_to_index.contains_key(&block.start_address);
        if op_idx >= block.ops.len() {
            return MalformedDefUseWindowDetail {
                relation: MalformedDefUseWindowRelation::OpIndexMissing,
                def_op_idx: op_idx,
                terminator_idx,
                consumer_count: 0,
                first_consumer_block: None,
                first_consumer_idx: None,
                first_consumer_op_seq: None,
                rhs_kind,
            };
        }
        let same_block_consumers =
            Self::collect_output_use_sites_in_block_unbounded(block, op_idx, output);
        let first_same_block_consumer = same_block_consumers.first().copied();
        let first_cross_block_consumer =
            self.first_output_use_site_outside_block(block.start_address, output);
        let relation = Self::classify_malformed_def_use_window_relation(
            op_idx,
            terminator_idx,
            first_same_block_consumer.map(|(idx, _)| idx),
            first_cross_block_consumer,
            block_index_present,
            Self::first_output_redefinition_in_block(block, op_idx, output).is_some(),
        );
        let consumer_count =
            same_block_consumers.len() + usize::from(first_cross_block_consumer.is_some());
        let (first_consumer_block, first_consumer_idx, first_consumer_op_seq) =
            if let Some((idx, op)) = first_same_block_consumer {
                (Some(block.start_address), Some(idx), Some(op.seq_num))
            } else if let Some((consumer_block, consumer_idx, consumer_op_seq)) =
                first_cross_block_consumer
            {
                (
                    Some(consumer_block),
                    Some(consumer_idx),
                    Some(consumer_op_seq),
                )
            } else {
                (None, None, None)
            };
        MalformedDefUseWindowDetail {
            relation,
            def_op_idx: op_idx,
            terminator_idx,
            consumer_count,
            first_consumer_block,
            first_consumer_idx,
            first_consumer_op_seq,
            rhs_kind,
        }
    }

    pub(super) fn describe_cross_block_consumer_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<(Option<u64>, Option<u32>, CrossBlockConsumerProvenance)> {
        let (consumer_block_addr, consumer_idx, consumer_op_seq) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let consumer_op = consumer_block.ops.get(consumer_idx)?;
        let consumer_is_multiequal = consumer_op.opcode == PcodeOpcode::MultiEqual;
        let immediate_successor = self
            .successors
            .get(def_block_idx)
            .is_some_and(|succs| succs.contains(&consumer_block_idx));
        let consumer_predecessor_count = self
            .predecessors
            .get(consumer_block_idx)
            .map_or(0, Vec::len);
        let consumer_is_join = consumer_predecessor_count > 1;
        let redefined_before_consumer =
            Self::first_output_redefinition_in_block(block, op_idx, output).is_some();
        let consumer_dominates_def = self.dom_tree.dominates(consumer_block_idx, def_block_idx);
        let consumer_postdominates_def = self
            .cfg_facts
            .postdominators()
            .postdominators()
            .get(&def_block_idx)
            .is_some_and(|set| set.contains(&consumer_block_idx));
        let relation = if consumer_is_multiequal {
            CrossBlockConsumerRelation::MergePhiConsumer
        } else if consumer_dominates_def && !consumer_postdominates_def {
            CrossBlockConsumerRelation::LoopBackedge
        } else if immediate_successor && !consumer_is_join {
            CrossBlockConsumerRelation::SuccessorBlock
        } else if consumer_is_join {
            CrossBlockConsumerRelation::JoinBlock
        } else if consumer_postdominates_def {
            CrossBlockConsumerRelation::PostDominatorBlock
        } else if immediate_successor {
            CrossBlockConsumerRelation::SuccessorBlock
        } else if self.address_to_index.contains_key(&consumer_block_addr) {
            CrossBlockConsumerRelation::OrdinaryDataConsumer
        } else {
            CrossBlockConsumerRelation::UnreachableOrUnclassified
        };
        Some((
            Some(consumer_block_addr),
            Some(consumer_op_seq),
            CrossBlockConsumerProvenance {
                relation,
                consumer_opcode: Some(consumer_op.opcode),
                consumer_is_multiequal,
                immediate_successor,
                consumer_is_join,
                redefined_before_consumer,
                def_successor_count: self.successors.get(def_block_idx).map_or(0, Vec::len),
                consumer_predecessor_count,
            },
        ))
    }

    pub(super) fn describe_cross_block_replacement_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<CrossBlockReplacementProof> {
        let (_, _, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let dominates_consumer = self.dom_tree.dominates(def_block_idx, consumer_block_idx);
        let rhs_low_cost = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let preserve_materialization = Self::should_preserve_materialized_expr(rhs);
        let no_redefinition_before_consumer =
            Self::first_output_redefinition_in_block(block, op_idx, output).is_none();
        let narrow_candidate = matches!(
            provenance.relation,
            CrossBlockConsumerRelation::SuccessorBlock
                | CrossBlockConsumerRelation::PostDominatorBlock
                | CrossBlockConsumerRelation::OrdinaryDataConsumer
        ) && provenance.def_successor_count == 1
            && !provenance.consumer_is_multiequal
            && rhs_low_cost
            && !preserve_materialization
            && no_redefinition_before_consumer
            && dominates_consumer;
        Some(CrossBlockReplacementProof {
            relation: provenance.relation,
            dominates_consumer,
            rhs_low_cost,
            preserve_materialization,
            no_redefinition_before_consumer,
            merge_phi: provenance.consumer_is_multiequal,
            def_successor_count: provenance.def_successor_count,
            consumer_predecessor_count: provenance.consumer_predecessor_count,
            narrow_candidate,
            consumer_opcode: provenance.consumer_opcode,
        })
    }

    pub(super) fn describe_cross_block_redefinition_detail(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        consumer_block_addr: Option<u64>,
    ) -> Option<CrossBlockRedefinitionDetail> {
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let consumer_block_addr = consumer_block_addr?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (consumer_idx, consumer_op) =
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
        let consumer_relation = self
            .describe_cross_block_consumer_provenance(block, op_idx, output)
            .map(|(_, _, provenance)| provenance.relation)
            .unwrap_or(CrossBlockConsumerRelation::UnreachableOrUnclassified);

        let terminator_idx = self.block_terminator_index(block);

        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block(block, op_idx, output)
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef,
                redef_block_addr: block.start_address,
                redef_op_idx: redef_idx,
                redef_op_seq: redef_op.seq_num,
                redef_opcode: redef_op.opcode,
                redef_rhs_kind: Self::classify_same_block_overwrite_rhs_kind(redef_op.opcode),
                overwrite_shape: Self::classify_same_block_overwrite_shape(
                    consumer_relation,
                    redef_idx,
                    redef_op.opcode,
                    terminator_idx,
                ),
                def_to_redef_gap: redef_idx.saturating_sub(op_idx),
                redef_to_terminator_gap: terminator_idx.map(|term| term.saturating_sub(redef_idx)),
            });
        }

        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block_from(consumer_block, 0, output)
        {
            if redef_idx < consumer_idx {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::RedefinedInConsumerBlockBeforeUse,
                    redef_block_addr: consumer_block.start_address,
                    redef_op_idx: redef_idx,
                    redef_op_seq: redef_op.seq_num,
                    redef_opcode: redef_op.opcode,
                    redef_rhs_kind: Self::classify_same_block_overwrite_rhs_kind(redef_op.opcode),
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                    def_to_redef_gap: redef_idx,
                    redef_to_terminator_gap: self
                        .block_terminator_index(consumer_block)
                        .map(|term| term.saturating_sub(redef_idx)),
                });
            }
        }

        if consumer_op.opcode == PcodeOpcode::MultiEqual {
            if let Some((pred_block_addr, redef_op_seq)) = self
                .predecessors
                .get(consumer_block_idx)
                .into_iter()
                .flat_map(|preds| preds.iter())
                .filter(|pred_idx| **pred_idx != def_block_idx)
                .find_map(|pred_idx| {
                    self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                        Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                            .map(|(_, op)| (pred_block.start_address, op.seq_num))
                    })
                })
            {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::PhiRedefinition,
                    redef_block_addr: pred_block_addr,
                    redef_op_idx: 0,
                    redef_op_seq,
                    redef_opcode: PcodeOpcode::MultiEqual,
                    redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                    def_to_redef_gap: 0,
                    redef_to_terminator_gap: None,
                });
            }
        }

        if self.dom_tree.dominates(consumer_block_idx, def_block_idx) {
            if let Some((redef_block_addr, redef_op_seq)) = self
                .predecessors
                .get(consumer_block_idx)
                .into_iter()
                .flat_map(|preds| preds.iter())
                .find_map(|pred_idx| {
                    self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                        Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                            .map(|(_, op)| (pred_block.start_address, op.seq_num))
                    })
                })
            {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::LoopCarriedRedefinition,
                    redef_block_addr,
                    redef_op_idx: 0,
                    redef_op_seq,
                    redef_opcode: PcodeOpcode::MultiEqual,
                    redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate,
                    def_to_redef_gap: 0,
                    redef_to_terminator_gap: None,
                });
            }
        }

        if let Some((edge_block_addr, redef_op_seq)) = self
            .successors
            .get(def_block_idx)
            .into_iter()
            .flat_map(|succs| succs.iter())
            .filter(|succ_idx| **succ_idx != consumer_block_idx)
            .find_map(|succ_idx| {
                self.pcode.blocks.get(*succ_idx).and_then(|succ_block| {
                    Self::first_output_redefinition_in_block_from(succ_block, 0, output)
                        .map(|(_, op)| (succ_block.start_address, op.seq_num))
                })
            })
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedOnEdge,
                redef_block_addr: edge_block_addr,
                redef_op_idx: 0,
                redef_op_seq,
                redef_opcode: PcodeOpcode::Copy,
                redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                def_to_redef_gap: 0,
                redef_to_terminator_gap: None,
            });
        }

        if let Some((pred_block_addr, redef_op_seq)) = self
            .predecessors
            .get(consumer_block_idx)
            .into_iter()
            .flat_map(|preds| preds.iter())
            .filter(|pred_idx| **pred_idx != def_block_idx)
            .find_map(|pred_idx| {
                self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                    Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                        .map(|(_, op)| (pred_block.start_address, op.seq_num))
                })
            })
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedInSiblingPredecessor,
                redef_block_addr: pred_block_addr,
                redef_op_idx: 0,
                redef_op_seq,
                redef_opcode: PcodeOpcode::Copy,
                redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                def_to_redef_gap: 0,
                redef_to_terminator_gap: None,
            });
        }

        self.pcode
            .blocks
            .iter()
            .filter(|candidate| {
                candidate.start_address != block.start_address
                    && candidate.start_address != consumer_block_addr
            })
            .find_map(|candidate| {
                Self::first_output_redefinition_in_block_from(candidate, 0, output).map(
                    |(_, op)| CrossBlockRedefinitionDetail {
                        relation: CrossBlockRedefinitionRelation::UnknownRedefinition,
                        redef_block_addr: candidate.start_address,
                        redef_op_idx: 0,
                        redef_op_seq: op.seq_num,
                        redef_opcode: op.opcode,
                        redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                        overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                        def_to_redef_gap: 0,
                        redef_to_terminator_gap: None,
                    },
                )
            })
    }

    pub(super) fn describe_copy_overwrite_restart_proof(
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
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
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

    pub(super) fn describe_predicate_overwrite_refresh_proof(
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
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
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

    pub(super) fn predicate_consumer_matches_output_guard_family(
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

    pub(super) fn ops_share_copylike_value(def_op: &PcodeOp, redef_op: &PcodeOp) -> bool {
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

    pub(super) fn format_copy_overwrite_inputs(inputs: &[Varnode]) -> String {
        let formatted = inputs
            .iter()
            .map(Self::format_copy_overwrite_varnode)
            .collect::<Vec<_>>()
            .join(",");
        format!("[{formatted}]")
    }

    pub(super) fn format_copy_overwrite_varnode(vn: &Varnode) -> String {
        if vn.is_constant {
            format!("const(0x{:x}:s{})", vn.offset, vn.size)
        } else {
            format!("space:{}:0x{:x}:s{}", vn.space_id, vn.offset, vn.size)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn malformed_def_use_window_relation_marks_terminator_missing() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0, None, None, None, true, true,
        );

        assert_eq!(relation, MalformedDefUseWindowRelation::TerminatorMissing);
    }

    #[test]
    fn malformed_def_use_window_relation_marks_cross_block_consumer() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0,
            Some(3),
            None,
            Some((0x2000, 1, 7)),
            true,
            true,
        );

        assert_eq!(
            relation,
            MalformedDefUseWindowRelation::ConsumerInDifferentBlock
        );
    }

    #[test]
    fn malformed_def_use_window_relation_marks_redefinition_before_consumer() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0,
            Some(3),
            None,
            None,
            true,
            true,
        );

        assert_eq!(
            relation,
            MalformedDefUseWindowRelation::RedefinitionBeforeConsumer
        );
    }

    #[test]
    fn cross_block_consumer_provenance_prefers_merge_phi_consumer() {
        let output = varnode(0x10);
        let rhs = HirExpr::Const(1, int(32));
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    1,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![constant(2)],
                )],
            ),
            block_at(
                0x1020,
                2,
                vec![op(
                    2,
                    PcodeOpcode::MultiEqual,
                    Some(varnode(0x30)),
                    vec![output.clone(), varnode(0x20)],
                )],
            ),
        ];
        blocks[0].successors = vec![2];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let provenance = builder
            .describe_cross_block_consumer_provenance(&blocks[0], 0, &output)
            .expect("cross-block provenance");

        assert_eq!(
            provenance.2.relation,
            CrossBlockConsumerRelation::MergePhiConsumer
        );
        assert!(provenance.2.consumer_is_multiequal);
        let proof = builder
            .describe_cross_block_replacement_proof(&blocks[0], 0, &output, &rhs)
            .expect("cross-block proof");
        assert!(!proof.narrow_candidate);
        assert!(proof.merge_phi);
    }

    #[test]
    fn cross_block_consumer_provenance_marks_single_successor_data_consumer() {
        let output = varnode(0x10);
        let rhs = HirExpr::Const(1, int(32));
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    1,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let provenance = builder
            .describe_cross_block_consumer_provenance(&blocks[0], 0, &output)
            .expect("cross-block provenance");

        assert_eq!(
            provenance.2.relation,
            CrossBlockConsumerRelation::SuccessorBlock
        );
        assert!(!provenance.2.consumer_is_multiequal);
        assert!(provenance.2.immediate_successor);
        assert!(!provenance.2.consumer_is_join);
        let proof = builder
            .describe_cross_block_replacement_proof(&blocks[0], 0, &output, &rhs)
            .expect("cross-block proof");
        assert!(proof.narrow_candidate);
        assert!(proof.dominates_consumer);
        assert!(proof.rhs_low_cost);
        assert!(proof.no_redefinition_before_consumer);
    }

    #[test]
    fn cross_block_redefinition_marks_def_block_after_def() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(2)],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    2,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");

        assert_eq!(
            redef.relation,
            CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
        );
        assert_eq!(redef.redef_block_addr, 0x1000);
        assert_eq!(redef.redef_op_seq, 1);
    }

    #[test]
    fn cross_block_redefinition_marks_consumer_block_before_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(2)],
                    ),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(varnode(0x20)),
                        vec![output.clone()],
                    ),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");

        assert_eq!(
            redef.relation,
            CrossBlockRedefinitionRelation::RedefinedInConsumerBlockBeforeUse
        );
        assert_eq!(redef.redef_block_addr, 0x1010);
        assert_eq!(redef.redef_op_seq, 1);
    }

    #[test]
    fn copy_overwrite_restart_proof_marks_same_value_and_no_pre_redef_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");
        assert_eq!(
            redef.overwrite_shape,
            SameBlockOverwriteShapeKind::OverwriteAtCopy
        );

        let proof = builder
            .describe_copy_overwrite_restart_proof(&blocks[0], 0, &output, &redef)
            .expect("copy overwrite proof");

        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
        assert_eq!(proof.consumer_block_addr, 0x1010);
        assert_eq!(proof.consumer_op_seq, 3);
        assert_eq!(proof.redef_rhs, "[const(0x1:s8)]");
    }

    #[test]
    fn def_window_restart_proves_copy_overwrite_successor_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
            .expect("restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::SuccessorBlock
        );
        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn def_window_restart_applies_for_copy_overwrite_postdominator_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1008)]),
                ],
            ),
            block_at(
                0x1008,
                1,
                vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1010,
                2,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
            .expect("restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::PostDominatorBlock
        );
        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn def_window_restart_rejects_pre_redef_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(varnode(0x20)),
                        vec![output.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x30)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(3), &output)
                .is_none()
        );
    }

    #[test]
    fn def_window_restart_rejects_impure_copy_overwrite_rhs() {
        let output = varnode(0x10);
        let ptr = varnode(0x11);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Load,
                        Some(output.clone()),
                        vec![constant(0), ptr.clone()],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
                .is_none()
        );
    }

    #[test]
    fn def_window_restart_rejects_alias_hazard_after_redef() {
        let output = varnode(0x10);
        let ptr = varnode(0x11);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        2,
                        PcodeOpcode::Load,
                        Some(varnode(0x20)),
                        vec![constant(0), ptr.clone()],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x30)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(3), &output)
                .is_none()
        );
    }

    #[test]
    fn predicate_overwrite_refresh_proof_marks_same_guard_family_for_bool_negate_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::BoolNegate,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("redef");

        let proof = builder
            .describe_predicate_overwrite_refresh_proof(
                &blocks[0],
                0,
                &output,
                &redef,
                CrossBlockConsumerRelation::PostDominatorBlock,
            )
            .expect("predicate proof");

        assert!(proof.same_guard_family);
        assert!(proof.redef_dominates_predicate);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn predicate_overwrite_refresh_proof_marks_non_guard_family_for_plain_copy_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("redef");

        let proof = builder
            .describe_predicate_overwrite_refresh_proof(
                &blocks[0],
                0,
                &output,
                &redef,
                CrossBlockConsumerRelation::SuccessorBlock,
            )
            .expect("predicate proof");

        assert!(!proof.same_guard_family);
        assert!(proof.redef_dominates_predicate);
    }

    #[test]
    fn predicate_refresh_restart_proves_same_guard_postdom_boolnegate() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1008)]),
                ],
            ),
            block_at(
                0x1008,
                1,
                vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1010,
                2,
                vec![op(
                    4,
                    PcodeOpcode::BoolNegate,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_predicate_refresh(&blocks[0], 0, Some(2), &output)
            .expect("predicate refresh restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::PostDominatorBlock
        );
        assert!(proof.same_guard_family);
        assert!(!proof.old_def_has_pre_redef_use);
        assert!(proof.redef_dominates_predicate);
    }

    #[test]
    fn predicate_refresh_restart_rejects_successor_int_notequal_composition() {
        let output = varnode(0x10);
        let other = varnode(0x12);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntSLess,
                        Some(output.clone()),
                        vec![varnode(0x13), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::IntNotEqual,
                    Some(varnode(0x20)),
                    vec![output.clone(), other],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_predicate_refresh(&blocks[0], 0, Some(2), &output)
                .is_none()
        );
    }

    #[test]
    fn forward_join_not_selected_proof_marks_current_heuristic_rejection() {
        let output = varnode(0x10);
        let consumer_out = varnode(0x30);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(consumer_out),
                        vec![output.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1010), varnode(0x40)],
                    ),
                ],
            ),
            block_at(
                0x1008,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        3,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1010), varnode(0x41)],
                    ),
                ],
            ),
            block_at(
                0x1010,
                2,
                vec![op(4, PcodeOpcode::Branch, None, vec![constant(0x1030)])],
            ),
            block_at(
                0x1020,
                3,
                vec![op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)])],
            ),
            block_at(
                0x1030,
                4,
                vec![op(
                    6,
                    PcodeOpcode::Copy,
                    Some(varnode(0x50)),
                    vec![varnode(0x60)],
                )],
            ),
            block_at(
                0x1050,
                5,
                vec![op(
                    7,
                    PcodeOpcode::Copy,
                    Some(varnode(0x61)),
                    vec![constant(0)],
                )],
            ),
        ];
        blocks[0].successors = vec![1, 5];
        blocks[1].successors = vec![2, 3];
        blocks[2].successors = vec![4];
        blocks[3].successors = vec![4];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Const(1, int(32));

        let proof = builder
            .describe_forward_join_not_selected_proof(&blocks[1], 0, &output, &rhs)
            .expect("forward join proof");

        assert!(proof.event_reaches_forward_join);
        assert_eq!(
            proof.rejected_reason,
            ForwardJoinNotSelectedRejectedReason::JoinRejectedByCurrentHeuristic
        );
    }

    #[test]
    fn forward_join_not_selected_classifier_marks_non_postdominating_join() {
        let pcode = pcode_function(vec![]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert_eq!(
            builder.classify_forward_join_not_selected_rejected_reason(
                0,
                1,
                true,
                Some(false),
                2,
                false,
            ),
            ForwardJoinNotSelectedRejectedReason::JoinDoesNotPostdominate
        );
    }

    #[test]
    fn ambiguous_join_pred_reason_prefers_all_incoming_same() {
        let reason = PreviewBuilder::classify_ambiguous_join_pred_reason(
            true,
            false,
            false,
            true,
            3,
            DisallowedSingleConsumerConsumerKind::StoreValue,
        );
        assert_eq!(reason, AmbiguousJoinPredReason::AllIncomingSame);
    }

    #[test]
    fn ambiguous_join_pred_reason_prefers_missing_incoming() {
        let reason = PreviewBuilder::classify_ambiguous_join_pred_reason(
            false,
            true,
            false,
            true,
            1,
            DisallowedSingleConsumerConsumerKind::StoreValue,
        );
        assert_eq!(reason, AmbiguousJoinPredReason::MissingIncomingForSomePred);
    }

    #[test]
    fn ambiguous_join_pred_reason_prefers_conflicting_incoming() {
        let reason = PreviewBuilder::classify_ambiguous_join_pred_reason(
            false,
            false,
            true,
            true,
            3,
            DisallowedSingleConsumerConsumerKind::OtherData,
        );
        assert_eq!(reason, AmbiguousJoinPredReason::ConflictingIncomingValues);
    }

    #[test]
    fn join_merge_missing_reason_prefers_all_incoming_same() {
        let reason = PreviewBuilder::classify_join_merge_missing_reason(
            true,
            false,
            false,
            2,
            DisallowedSingleConsumerConsumerKind::OtherData,
        );
        assert_eq!(reason, JoinMergeMissingReason::AllIncomingSame);
    }

    #[test]
    fn join_merge_missing_reason_prefers_missing_incoming() {
        let reason = PreviewBuilder::classify_join_merge_missing_reason(
            false,
            true,
            true,
            1,
            DisallowedSingleConsumerConsumerKind::OtherData,
        );
        assert_eq!(reason, JoinMergeMissingReason::MissingIncomingForSomePred);
    }

    #[test]
    fn join_merge_missing_reason_prefers_conflicting_incoming() {
        let reason = PreviewBuilder::classify_join_merge_missing_reason(
            false,
            false,
            true,
            2,
            DisallowedSingleConsumerConsumerKind::StoreValue,
        );
        assert_eq!(reason, JoinMergeMissingReason::ConflictingIncomingValues);
    }

    #[test]
    fn merge_binding_candidate_result_prefers_missing_incoming_semantics() {
        let kinds = BTreeSet::from([MergeBindingCandidateIncomingKind::VarOrConst]);
        let result = PreviewBuilder::classify_merge_binding_candidate_result(
            1,
            1,
            &kinds,
            DisallowedSingleConsumerConsumerKind::OtherData,
        );
        assert_eq!(
            result,
            MergeBindingCandidateResult::MissingIncomingSemanticsRequired
        );
    }

    #[test]
    fn merge_binding_candidate_result_marks_phi_like_candidate_for_safe_conflicts() {
        let kinds = BTreeSet::from([
            MergeBindingCandidateIncomingKind::VarOrConst,
            MergeBindingCandidateIncomingKind::Arithmetic,
        ]);
        let result = PreviewBuilder::classify_merge_binding_candidate_result(
            0,
            1,
            &kinds,
            DisallowedSingleConsumerConsumerKind::OtherData,
        );
        assert_eq!(result, MergeBindingCandidateResult::PhiLikeBindingCandidate);
    }

    #[test]
    fn missing_incoming_pred_kind_prefers_entry_default() {
        let kind =
            PreviewBuilder::classify_missing_incoming_pred_kind(true, true, false, false, false);
        assert_eq!(kind, MissingIncomingPredKind::MissingBecauseEntryDefault);
    }

    #[test]
    fn missing_incoming_pred_kind_prefers_prior_def_dominates() {
        let kind =
            PreviewBuilder::classify_missing_incoming_pred_kind(false, true, false, true, true);
        assert_eq!(
            kind,
            MissingIncomingPredKind::MissingBecausePriorDefDominates
        );
    }

    #[test]
    fn missing_incoming_pred_kind_prefers_path_sensitive_prior_def() {
        let kind =
            PreviewBuilder::classify_missing_incoming_pred_kind(false, true, false, true, false);
        assert_eq!(kind, MissingIncomingPredKind::MissingBecausePathSensitive);
    }

    #[test]
    fn dominating_prior_def_proof_result_prefers_stable_to_merge() {
        let result =
            PreviewBuilder::classify_dominating_prior_def_proof_result(true, false, false, false);
        assert_eq!(result, DominatingPriorDefProofResult::PriorDefStableToMerge);
    }

    #[test]
    fn dominating_prior_def_proof_result_marks_redefinition_before_merge() {
        let result =
            PreviewBuilder::classify_dominating_prior_def_proof_result(true, true, false, false);
        assert_eq!(
            result,
            DominatingPriorDefProofResult::PriorDefRedefinedBeforeMerge
        );
    }

    #[test]
    fn dominating_prior_def_proof_result_marks_non_dominating_merge() {
        let result =
            PreviewBuilder::classify_dominating_prior_def_proof_result(false, false, false, false);
        assert_eq!(
            result,
            DominatingPriorDefProofResult::PriorDefDoesNotDominateMerge
        );
    }

    #[test]
    fn missing_no_prior_def_reason_marks_entry_default() {
        let (reason, default_candidate) = PreviewBuilder::classify_missing_no_prior_def_reason(
            true,
            false,
            &Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: 0x10,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        );
        assert_eq!(reason, MissingNoPriorDefReason::EntryDefaultCandidate);
        assert_eq!(default_candidate, "entry-default");
    }

    #[test]
    fn missing_no_prior_def_reason_marks_register_default() {
        let (reason, default_candidate) = PreviewBuilder::classify_missing_no_prior_def_reason(
            false,
            false,
            &Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x20,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
        );
        assert_eq!(reason, MissingNoPriorDefReason::RegisterDefault);
        assert_eq!(default_candidate, "register-default");
    }

    #[test]
    fn missing_no_prior_def_reason_marks_temp_only() {
        let (reason, default_candidate) = PreviewBuilder::classify_missing_no_prior_def_reason(
            false,
            false,
            &Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: 0x44,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        );
        assert_eq!(reason, MissingNoPriorDefReason::TempOnlyNoDef);
        assert_eq!(default_candidate, "temp-only");
    }

    #[test]
    fn temp_only_representative_reason_marks_root_attributed() {
        let reason = PreviewBuilder::classify_temp_only_representative_reason(
            true,
            false,
            DisallowedSingleConsumerConsumerKind::OtherData,
            "SyntheticRootBlock",
        );
        assert_eq!(reason, TempOnlyRepresentativeReason::RootAttributedTemp);
    }

    #[test]
    fn temp_only_representative_reason_marks_dead_pred() {
        let reason = PreviewBuilder::classify_temp_only_representative_reason(
            false,
            true,
            DisallowedSingleConsumerConsumerKind::StoreValue,
            "DeadPredNoDef",
        );
        assert_eq!(reason, TempOnlyRepresentativeReason::DeadTempRepresentative);
    }

    #[test]
    fn temp_only_representative_reason_marks_temp_residue() {
        let reason = PreviewBuilder::classify_temp_only_representative_reason(
            false,
            false,
            DisallowedSingleConsumerConsumerKind::OtherData,
            "TempOnlyNoDef",
        );
        assert_eq!(
            reason,
            TempOnlyRepresentativeReason::TempRepresentativeResidue
        );
    }
}
