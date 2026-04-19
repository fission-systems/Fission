use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
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
}
