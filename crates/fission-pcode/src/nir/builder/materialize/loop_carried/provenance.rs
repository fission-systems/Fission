use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn describe_loop_carried_overwrite_provenance(
        &self,
        _block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) -> Option<LoopCarriedOverwriteProvenance> {
        if redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate {
            return None;
        }
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let redef_block_idx = self
            .address_to_index
            .get(&redef.redef_block_addr)
            .copied()?;
        let redef_block = self.pcode.blocks.get(redef_block_idx)?;
        let loop_header = consumer_block_addr;
        let backedge_block = redef.redef_block_addr;
        let (has_multiequal, phi_input_count) = consumer_block
            .ops
            .iter()
            .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
            .fold((false, 0usize), |(_, max_inputs), op| {
                (true, max_inputs.max(op.inputs.len()))
            });
        let redef_op = redef_block.ops.get(redef.redef_op_idx)?;
        let redef_rhs = Self::format_copy_overwrite_inputs(&redef_op.inputs);
        let carried_value_kind =
            Self::classify_loop_carried_value_kind(output, redef_op, self.options.pointer_size);
        let induction_like = matches!(
            carried_value_kind,
            LoopCarriedValueKind::CounterIncrement | LoopCarriedValueKind::PointerAdvance
        );
        Some(LoopCarriedOverwriteProvenance {
            loop_header,
            backedge_block,
            consumer_block: consumer_block_addr,
            consumer_op_seq,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs,
            has_multiequal,
            phi_input_count,
            induction_like,
            carried_value_kind,
        })
    }

    pub(in crate::nir::builder) fn describe_loop_boolean_flag_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) -> Option<LoopBooleanFlagProof> {
        let provenance = self.describe_loop_carried_overwrite_provenance(
            block,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        )?;
        if provenance.carried_value_kind != LoopCarriedValueKind::BooleanFlag {
            return None;
        }
        let loop_header_idx = self
            .address_to_index
            .get(&provenance.loop_header)
            .copied()?;
        let loop_header = self.pcode.blocks.get(loop_header_idx)?;
        let backedge_block_idx = self
            .address_to_index
            .get(&provenance.backedge_block)
            .copied()?;
        let backedge_block = self.pcode.blocks.get(backedge_block_idx)?;
        let consumer_op = loop_header
            .ops
            .iter()
            .find(|op| op.seq_num == consumer_op_seq)?;
        let guard_family = Self::classify_loop_boolean_guard_family(consumer_op, output);
        let old_def_has_pre_redef_use =
            !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty();
        let loop_header_terminator = self.block_terminator_index(loop_header)?;
        let terminator_op = loop_header.ops.get(loop_header_terminator)?;
        let consumer_is_loop_header_predicate = if consumer_op.seq_num == terminator_op.seq_num {
            Self::classify_loop_boolean_guard_family(consumer_op, output)
                != LoopBooleanGuardFamily::NonPredicate
        } else {
            terminator_op.opcode == PcodeOpcode::CBranch
                && terminator_op
                    .inputs
                    .get(1)
                    .zip(consumer_op.output.as_ref())
                    .is_some_and(|(cond, consumer_output)| {
                        VarnodeKey::from(cond) == VarnodeKey::from(consumer_output)
                    })
        };
        let same_guard_as_exit = consumer_is_loop_header_predicate
            && guard_family != LoopBooleanGuardFamily::NonPredicate;
        let (exit_edge, backedge_edge) =
            self.describe_loop_header_edges(loop_header_idx, backedge_block_idx);
        let redef_dominates_backedge =
            self.block_terminator_index(backedge_block)
                .is_some_and(|term_idx| {
                    backedge_block.ops.get(term_idx).is_some_and(|term_op| {
                        term_op.opcode == PcodeOpcode::Branch
                            && term_op.inputs.first().is_some_and(|target| {
                                target.is_constant
                                    && target.constant_val as u64 == provenance.loop_header
                            })
                    }) && redef.redef_op_idx < term_idx
                });
        Some(LoopBooleanFlagProof {
            consumer_opcode: consumer_op.opcode,
            exit_edge,
            backedge_edge,
            guard_family,
            same_guard_as_exit,
            old_def_has_pre_redef_use,
            redef_dominates_backedge,
            consumer_is_loop_header_predicate,
        })
    }

    pub(in crate::nir::builder) fn describe_loop_guard_refresh_dominance_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) -> Option<LoopGuardRefreshDominanceProof> {
        let boolean_proof = self.describe_loop_boolean_flag_proof(
            block,
            op_idx,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        )?;
        if !boolean_proof.same_guard_as_exit || !boolean_proof.consumer_is_loop_header_predicate {
            return None;
        }

        let loop_header_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let backedge_block_idx = self
            .address_to_index
            .get(&redef.redef_block_addr)
            .copied()?;
        let loop_header_preds = self.predecessors.get(loop_header_idx)?;
        let header_predicate_uses_redef = boolean_proof.consumer_is_loop_header_predicate;

        let Some(backedge_term_idx) = self
            .pcode
            .blocks
            .get(backedge_block_idx)
            .and_then(|block| self.block_terminator_index(block))
        else {
            return Some(LoopGuardRefreshDominanceProof {
                redef_block: redef.redef_block_addr,
                backedge_block: redef.redef_block_addr,
                redef_before_backedge_branch: false,
                all_backedge_paths_covered: false,
                header_predicate_uses_redef,
                reason: LoopGuardRefreshDominanceReason::MissingBackedgeTerminator,
            });
        };

        let backedge_block = self.pcode.blocks.get(backedge_block_idx)?;
        let backedge_term = backedge_block.ops.get(backedge_term_idx)?;
        let redef_before_backedge_branch = backedge_term.opcode == PcodeOpcode::Branch
            && backedge_term.inputs.first().is_some_and(|target| {
                target.is_constant && target.constant_val as u64 == consumer_block_addr
            })
            && redef.redef_op_idx < backedge_term_idx;

        let all_backedge_paths_covered = loop_header_preds.len() == 1
            && loop_header_preds
                .first()
                .is_some_and(|pred_idx| *pred_idx == backedge_block_idx);

        let reason = if !header_predicate_uses_redef {
            LoopGuardRefreshDominanceReason::HeaderPredicateUsesIntermediate
        } else if !loop_header_preds.contains(&backedge_block_idx) {
            LoopGuardRefreshDominanceReason::RedefInNonBackedgeBlock
        } else if !redef_before_backedge_branch {
            LoopGuardRefreshDominanceReason::RedefAfterBackedgeBranch
        } else if !all_backedge_paths_covered {
            LoopGuardRefreshDominanceReason::MultipleBackedgeBlocks
        } else if boolean_proof.redef_dominates_backedge {
            LoopGuardRefreshDominanceReason::ProvedBySingleBackedge
        } else {
            LoopGuardRefreshDominanceReason::UnknownDominance
        };

        Some(LoopGuardRefreshDominanceProof {
            redef_block: redef.redef_block_addr,
            backedge_block: redef.redef_block_addr,
            redef_before_backedge_branch,
            all_backedge_paths_covered,
            header_predicate_uses_redef,
            reason,
        })
    }

    pub(in crate::nir::builder) fn describe_loop_boundary_binding_correlation(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        reason: MaterializationRejectionReason,
    ) -> Option<LoopBoundaryBindingCorrelation> {
        let (consumer_block_addr, consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
        if provenance.relation != CrossBlockConsumerRelation::LoopBackedge {
            return None;
        }
        let consumer_block_addr = consumer_block_addr?;
        let consumer_op_seq = consumer_op_seq?;
        let redef = self.describe_cross_block_redefinition_detail(
            block,
            op_idx,
            output,
            Some(consumer_block_addr),
        )?;
        let carried = self.describe_loop_carried_overwrite_provenance(
            block,
            output,
            &redef,
            consumer_block_addr,
            consumer_op_seq,
        )?;
        if carried.carried_value_kind != LoopCarriedValueKind::BooleanFlag {
            return None;
        }
        let proof = self.describe_loop_boolean_flag_proof(
            block,
            op_idx,
            output,
            &redef,
            consumer_block_addr,
            consumer_op_seq,
        )?;
        let family = match proof.consumer_opcode {
            PcodeOpcode::BoolNegate => LoopBoundaryBindingFamily::BoolNegate,
            PcodeOpcode::IntNotEqual => LoopBoundaryBindingFamily::IntNotEqual,
            _ => LoopBoundaryBindingFamily::OtherBooleanFlag,
        };
        let existing_binding = self
            .materialized_vns
            .get(&MaterializedVarnodeKey::new(output, &block.ops[op_idx]))
            .cloned();
        let candidate_binding = format!(
            "loop_header_0x{:x}_space{}_off0x{:x}",
            carried.loop_header, output.space_id, output.offset
        );
        Some(LoopBoundaryBindingCorrelation {
            loop_header: carried.loop_header,
            family,
            missing_merge_binding: reason == MaterializationRejectionReason::MissingMergeBinding,
            stable_representative_required: reason
                == MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
            merge_block: Some(carried.loop_header),
            candidate_binding,
            existing_binding,
        })
    }

    pub(in crate::nir::builder) fn format_redefinition_rhs(&self, redef: &CrossBlockRedefinitionDetail) -> String {
        let Some(redef_block_idx) = self.address_to_index.get(&redef.redef_block_addr).copied()
        else {
            return "<unknown>".to_string();
        };
        let Some(redef_block) = self.pcode.blocks.get(redef_block_idx) else {
            return "<unknown>".to_string();
        };
        let Some(redef_op) = redef_block.ops.get(redef.redef_op_idx) else {
            return "<unknown>".to_string();
        };
        Self::format_copy_overwrite_inputs(&redef_op.inputs)
    }

    pub(in crate::nir::builder) fn classify_loop_boolean_guard_family(
        op: &PcodeOp,
        output: &Varnode,
    ) -> LoopBooleanGuardFamily {
        let key = VarnodeKey::from(output);
        match op.opcode {
            PcodeOpcode::BoolNegate => op
                .inputs
                .first()
                .is_some_and(|input| VarnodeKey::from(input) == key)
                .then_some(LoopBooleanGuardFamily::NegatedFlag)
                .unwrap_or(LoopBooleanGuardFamily::NonPredicate),
            PcodeOpcode::IntEqual => {
                if op.inputs.len() != 2 {
                    return LoopBooleanGuardFamily::NonPredicate;
                }
                let lhs_matches = VarnodeKey::from(&op.inputs[0]) == key
                    && op.inputs[1].is_constant
                    && op.inputs[1].constant_val == 0;
                let rhs_matches = VarnodeKey::from(&op.inputs[1]) == key
                    && op.inputs[0].is_constant
                    && op.inputs[0].constant_val == 0;
                if lhs_matches || rhs_matches {
                    LoopBooleanGuardFamily::EqZero
                } else {
                    LoopBooleanGuardFamily::NonPredicate
                }
            }
            PcodeOpcode::IntNotEqual => {
                if op.inputs.len() != 2 {
                    return LoopBooleanGuardFamily::NonPredicate;
                }
                let lhs_matches = VarnodeKey::from(&op.inputs[0]) == key
                    && op.inputs[1].is_constant
                    && op.inputs[1].constant_val == 0;
                let rhs_matches = VarnodeKey::from(&op.inputs[1]) == key
                    && op.inputs[0].is_constant
                    && op.inputs[0].constant_val == 0;
                if lhs_matches || rhs_matches {
                    LoopBooleanGuardFamily::NeZero
                } else {
                    LoopBooleanGuardFamily::NonPredicate
                }
            }
            PcodeOpcode::CBranch => op
                .inputs
                .get(1)
                .is_some_and(|input| VarnodeKey::from(input) == key)
                .then_some(LoopBooleanGuardFamily::DirectFlag)
                .unwrap_or(LoopBooleanGuardFamily::NonPredicate),
            _ => LoopBooleanGuardFamily::NonPredicate,
        }
    }

    pub(in crate::nir::builder) fn describe_loop_header_edges(
        &self,
        loop_header_idx: usize,
        backedge_block_idx: usize,
    ) -> (Option<u64>, Option<u64>) {
        let Some(successors) = self.successors.get(loop_header_idx) else {
            return (None, None);
        };
        let mut backedge_edge = None;
        let mut exit_edge = None;
        for succ_idx in successors {
            let succ_addr = self
                .pcode
                .blocks
                .get(*succ_idx)
                .map(|block| block.start_address);
            if self.block_can_reach(*succ_idx, backedge_block_idx, loop_header_idx) {
                backedge_edge = succ_addr;
            } else if exit_edge.is_none() {
                exit_edge = succ_addr;
            }
        }
        (exit_edge, backedge_edge)
    }

    pub(in crate::nir::builder) fn block_can_reach(
        &self,
        start_idx: usize,
        target_idx: usize,
        stop_idx: usize,
    ) -> bool {
        let cache_key = (start_idx, target_idx, stop_idx);
        if let Some(cached) = self.reachability_cache.borrow().get(&cache_key).copied() {
            return cached;
        }
        let reachable = self.block_can_reach_uncached(start_idx, target_idx, stop_idx);
        self.reachability_cache
            .borrow_mut()
            .insert(cache_key, reachable);
        reachable
    }

    fn block_can_reach_uncached(
        &self,
        start_idx: usize,
        target_idx: usize,
        stop_idx: usize,
    ) -> bool {
        if start_idx == target_idx {
            return true;
        }
        let mut stack = vec![start_idx];
        let mut visited = HashSet::new();
        while let Some(idx) = stack.pop() {
            if !visited.insert(idx) {
                continue;
            }
            if idx == target_idx {
                return true;
            }
            if idx == stop_idx && idx != start_idx {
                continue;
            }
            if let Some(succs) = self.successors.get(idx) {
                for succ in succs {
                    if !visited.contains(succ) {
                        stack.push(*succ);
                    }
                }
            }
        }
        false
    }

    pub(in crate::nir::builder) fn classify_loop_carried_value_kind(
        output: &Varnode,
        redef_op: &PcodeOp,
        pointer_size: u32,
    ) -> LoopCarriedValueKind {
        match redef_op.opcode {
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow => LoopCarriedValueKind::BooleanFlag,
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub => {
                let self_carried = redef_op
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output));
                let has_const = redef_op.inputs.iter().any(|input| input.is_constant);
                if self_carried && has_const && output.size == pointer_size {
                    LoopCarriedValueKind::PointerAdvance
                } else if self_carried && has_const {
                    LoopCarriedValueKind::CounterIncrement
                } else if self_carried {
                    LoopCarriedValueKind::Accumulator
                } else {
                    LoopCarriedValueKind::UnknownLoopCarried
                }
            }
            PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => LoopCarriedValueKind::Accumulator,
            _ => LoopCarriedValueKind::UnknownLoopCarried,
        }
    }
}
