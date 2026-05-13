use super::contracts::*;
use super::*;

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
        if self.abi_state().param_slot_for_varnode(output).is_some()
            && !self.loop_carried_output_has_prior_definition(output)
        {
            return self.register_param(output);
        }
        None
    }

    fn loop_carried_output_has_prior_definition(&self, output: &Varnode) -> bool {
        self.lookup_def_site(output).is_some()
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

    fn is_loop_carried_register_update_candidate(output: &Varnode) -> bool {
        !output.is_constant && is_register_space_id(output.space_id) && output.size >= 4
    }

    fn prior_materialized_loop_carried_output_name(&self, output: &Varnode) -> Option<String> {
        let (site, op) = self.lookup_def_site(output)?;
        if Some(site) == self.current_lowering_site {
            return None;
        }
        let prior_output = op.output.as_ref()?;
        if VarnodeKey::from(prior_output) != VarnodeKey::from(output)
            && !Self::prior_output_aliases_loop_carried_update(prior_output, output)
        {
            return None;
        }
        self.materialized_vns
            .get(&MaterializedVarnodeKey::new(prior_output, op))
            .cloned()
    }

    fn prior_output_aliases_loop_carried_update(prior: &Varnode, current: &Varnode) -> bool {
        !prior.is_constant
            && !current.is_constant
            && prior.space_id == current.space_id
            && is_register_space_id(prior.space_id)
            && prior.offset == current.offset
            && prior.size == 8
            && current.size == 4
            && (x64_ghidra_reg_name(prior.offset).is_some()
                || aarch64_ghidra_reg_name(prior.offset, prior.size).is_some())
    }

    fn output_is_loop_carried_register_update(
        &self,
        block_idx: usize,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        let output_key = VarnodeKey::from(output);
        self.loop_bodies.iter().any(|loop_body| {
            loop_body.body.contains(&block_idx)
                && self.loop_update_reaches_backedge_tail(block_idx, loop_body)
                && (Self::op_reads_varnode_key(op, &output_key)
                    || self.loop_reads_varnode_before_update(
                        loop_body,
                        block_idx,
                        op_idx,
                        &output_key,
                    ))
        })
    }

    fn loop_update_reaches_backedge_tail(
        &self,
        block_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
    ) -> bool {
        loop_body.tails.iter().any(|tail| {
            *tail == block_idx || self.block_can_reach(block_idx, *tail, loop_body.head)
        })
    }

    fn loop_reads_varnode_before_update(
        &self,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        block_idx: usize,
        op_idx: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return false;
        };
        if Self::block_reads_varnode_before_redefinition(block, op_idx, output_key) {
            return true;
        }

        if loop_body.head == block_idx {
            return false;
        }
        self.pcode.blocks.get(loop_body.head).is_some_and(|head| {
            Self::block_reads_varnode_before_redefinition(head, head.ops.len(), output_key)
        })
    }

    fn block_reads_varnode_before_redefinition(
        block: &crate::pcode::PcodeBasicBlock,
        limit: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        for candidate in block.ops.iter().take(limit) {
            if Self::op_reads_varnode_key(candidate, output_key) {
                return true;
            }
            if candidate.output.as_ref().is_some_and(|output| {
                Self::varnode_key_may_alias_output(&VarnodeKey::from(output), output_key)
            }) {
                return false;
            }
        }
        false
    }

    fn op_reads_varnode_key(op: &PcodeOp, output_key: &VarnodeKey) -> bool {
        op.inputs
            .iter()
            .any(|input| Self::varnode_key_may_alias_output(&VarnodeKey::from(input), output_key))
    }

    fn varnode_key_may_alias_output(candidate: &VarnodeKey, output_key: &VarnodeKey) -> bool {
        candidate == output_key
            || (is_register_space_id(candidate.space_id)
                && is_register_space_id(output_key.space_id)
                && candidate.space_id == output_key.space_id
                && Self::register_key_ranges_overlap(candidate, output_key))
    }

    fn register_key_ranges_overlap(lhs: &VarnodeKey, rhs: &VarnodeKey) -> bool {
        let Some(lhs_end) = lhs.offset.checked_add(u64::from(lhs.size)) else {
            return false;
        };
        let Some(rhs_end) = rhs.offset.checked_add(u64::from(rhs.size)) else {
            return false;
        };
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }

    pub(super) fn describe_loop_carried_overwrite_provenance(
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

    pub(super) fn describe_loop_boolean_flag_proof(
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

    pub(super) fn describe_loop_guard_refresh_dominance_proof(
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

    pub(super) fn describe_loop_boundary_binding_correlation(
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

    pub(super) fn format_redefinition_rhs(&self, redef: &CrossBlockRedefinitionDetail) -> String {
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

    pub(super) fn classify_loop_boolean_guard_family(
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

    pub(super) fn describe_loop_header_edges(
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

    pub(super) fn block_can_reach(
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

    pub(super) fn classify_loop_carried_value_kind(
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

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn lhs_var(stmt: &HirStmt) -> Option<&str> {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } => Some(name.as_str()),
            _ => None,
        }
    }

    fn expr_var(expr: &HirExpr) -> Option<&str> {
        match expr {
            HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name.as_str()),
            HirExpr::Cast { expr, .. } => expr_var(expr),
            _ => None,
        }
    }

    fn expr_contains_shr(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Binary {
                op: HirBinaryOp::Shr,
                ..
            } => true,
            HirExpr::Binary { lhs, rhs, .. } => expr_contains_shr(lhs) || expr_contains_shr(rhs),
            HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => expr_contains_shr(expr),
            HirExpr::Call { args, .. } => args.iter().any(expr_contains_shr),
            HirExpr::Load { ptr, .. } => expr_contains_shr(ptr),
            HirExpr::PtrOffset { base, .. } => expr_contains_shr(base),
            HirExpr::Index { base, index, .. } => {
                expr_contains_shr(base) || expr_contains_shr(index)
            }
            HirExpr::AggregateCopy { src, .. } => expr_contains_shr(src),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                expr_contains_shr(cond)
                    || expr_contains_shr(then_expr)
                    || expr_contains_shr(else_expr)
            }
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        }
    }

    #[test]
    fn loop_carried_register_update_reuses_prior_binding_and_param() {
        let rax = reg(0x00, 8);
        let rcx = reg(0x08, 8);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(0)]),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(rax.clone()),
                        vec![rax.clone(), constant(1)],
                    ),
                    op(
                        3,
                        PcodeOpcode::IntAdd,
                        Some(rcx.clone()),
                        vec![rcx.clone(), constant(4)],
                    ),
                    op(4, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(0x1020, 2, vec![op(5, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![1, 2];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::WindowsX64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let preheader = builder
            .lower_block_stmts(&pcode.blocks[0])
            .expect("preheader lowering");
        let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("loop lowering");

        assert!(
            loop_body
                .iter()
                .any(|stmt| lhs_var(stmt) == Some(init_name)),
            "loop-carried accumulator update should reuse the preheader binding: {loop_body:?}"
        );
        assert!(
            loop_body
                .iter()
                .any(|stmt| lhs_var(stmt) == Some("param_1")),
            "loop-carried parameter register update should assign back to the parameter: {loop_body:?}"
        );
    }

    #[test]
    fn loop_carried_register_update_does_not_promote_prior_defined_abi_scratch() {
        let rdx = reg(0x10, 8);
        let mut blocks = vec![block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(rdx.clone()), vec![constant(5)]),
                op(
                    1,
                    PcodeOpcode::IntAdd,
                    Some(rdx.clone()),
                    vec![rdx.clone(), constant(1)],
                ),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
            ],
        )];
        blocks[0].successors = vec![0];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::WindowsX64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 0,
            op_idx: 1,
        });

        let name = builder.loop_carried_output_binding_name(
            &pcode.blocks[0],
            1,
            &pcode.blocks[0].ops[1],
            &rdx,
        );

        assert_ne!(name.as_deref(), Some("param_2"));
        assert!(
            name.is_none(),
            "prior-defined ABI scratch should not be promoted to param_2: {name:?}"
        );
    }

    #[test]
    fn loop_carried_register_update_reuses_wide_prior_for_gpr32_update() {
        let rax = reg(0x00, 8);
        let eax = reg(0x00, 4);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(rax), vec![constant(0)]),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(eax.clone()),
                        vec![eax, constant(1)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![1];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let preheader = builder
            .lower_block_stmts(&pcode.blocks[0])
            .expect("preheader lowering");
        let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("loop lowering");

        assert!(
            loop_body
                .iter()
                .any(|stmt| lhs_var(stmt) == Some(init_name)),
            "32-bit loop update should reuse the 64-bit zero initializer binding: {loop_body:?}"
        );
    }

    #[test]
    fn aarch64_loop_carried_register_update_reuses_wide_prior_for_w_gpr_update() {
        let x20 = reg(0x40a0, 8);
        let w20 = reg(0x40a0, 4);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(x20), vec![constant(0)]),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(w20.clone()),
                        vec![w20, constant(1)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![1];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let preheader = builder
            .lower_block_stmts(&pcode.blocks[0])
            .expect("preheader lowering");
        let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("loop lowering");

        assert!(
            loop_body
                .iter()
                .any(|stmt| lhs_var(stmt) == Some(init_name)),
            "AArch64 W-register loop update should reuse the X-register initializer binding: {loop_body:?}"
        );
    }

    #[test]
    fn aarch64_loop_carried_register_passthrough_update_reuses_prior_binding() {
        let x1 = reg(0x4048, 8);
        let w1 = reg(0x4048, 4);
        let shifted = varnode(0x50);
        let cond = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: 0x60,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(x1.clone()), vec![constant(11)]),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntRight,
                        Some(shifted.clone()),
                        vec![w1.clone(), constant(1)],
                    ),
                    op(3, PcodeOpcode::IntZExt, Some(x1.clone()), vec![shifted]),
                    op(
                        4,
                        PcodeOpcode::IntNotEqual,
                        Some(cond.clone()),
                        vec![w1, constant(0)],
                    ),
                    op(5, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
                ],
            ),
            block_at(0x1020, 2, vec![op(6, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![1, 2];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let preheader = builder
            .lower_block_stmts(&pcode.blocks[0])
            .expect("preheader lowering");
        let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("loop lowering");

        assert!(
            loop_body
                .iter()
                .any(|stmt| lhs_var(stmt) == Some(init_name)),
            "AArch64 passthrough loop update should assign back to the prior binding: {loop_body:?}"
        );
    }

    #[test]
    fn aarch64_loop_exit_register_read_reuses_loop_carried_binding_with_zero_bypass() {
        let x20 = reg(0x40a0, 8);
        let w20 = reg(0x40a0, 4);
        let zero = varnode(0x10);
        let sum = varnode(0x20);
        let out = varnode(0x30);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1020), constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(1, PcodeOpcode::Copy, Some(zero.clone()), vec![constant(0)]),
                    op(
                        2,
                        PcodeOpcode::IntZExt,
                        Some(x20.clone()),
                        vec![zero.clone()],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(4, PcodeOpcode::Copy, Some(zero.clone()), vec![constant(0)]),
                    op(5, PcodeOpcode::IntZExt, Some(x20.clone()), vec![zero]),
                    op(
                        6,
                        PcodeOpcode::IntAdd,
                        Some(sum.clone()),
                        vec![w20.clone(), constant(1)],
                    ),
                    op(7, PcodeOpcode::IntZExt, Some(x20.clone()), vec![sum]),
                    op(
                        8,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1020), constant(1)],
                    ),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![op(9, PcodeOpcode::Copy, Some(out), vec![w20.clone()])],
            ),
        ];
        blocks[0].successors = vec![2, 1];
        blocks[1].successors = vec![3];
        blocks[2].successors = vec![2, 3];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let bypass = builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("bypass lowering");
        assert!(!bypass.is_empty(), "bypass zero should materialize");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[2])
            .expect("loop lowering");
        let carried_name = loop_body
            .iter()
            .filter_map(lhs_var)
            .last()
            .expect("loop-carried binding")
            .to_string();

        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 3,
            op_idx: 0,
        });
        let mut visiting = std::collections::HashSet::new();
        let resolved = builder
            .lower_varnode(&w20, &mut visiting)
            .expect("exit register lowering");

        assert_eq!(expr_var(&resolved), Some(carried_name.as_str()));
        assert!(
            builder
                .temps
                .get(&carried_name)
                .and_then(|binding| binding.initializer.as_ref())
                .is_some_and(|initializer| matches!(initializer, HirExpr::Const(0, _))),
            "loop-carried exit binding should be initialized for the bypass path"
        );
    }

    #[test]
    fn aarch64_be_loop_exit_register_read_uses_low_w_view_of_x_binding() {
        let x20 = reg(0x40a0, 8);
        let w20_be = reg(0x40a4, 4);
        let zero = varnode(0x10);
        let sum = varnode(0x20);
        let out = varnode(0x30);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1020), constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(1, PcodeOpcode::Copy, Some(zero.clone()), vec![constant(0)]),
                    op(
                        2,
                        PcodeOpcode::IntZExt,
                        Some(x20.clone()),
                        vec![zero.clone()],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(4, PcodeOpcode::Copy, Some(zero.clone()), vec![constant(0)]),
                    op(5, PcodeOpcode::IntZExt, Some(x20.clone()), vec![zero]),
                    op(
                        6,
                        PcodeOpcode::IntAdd,
                        Some(sum.clone()),
                        vec![w20_be.clone(), constant(1)],
                    ),
                    op(7, PcodeOpcode::IntZExt, Some(x20.clone()), vec![sum]),
                    op(
                        8,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1020), constant(1)],
                    ),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![op(9, PcodeOpcode::Copy, Some(out), vec![w20_be.clone()])],
            ),
        ];
        blocks[0].successors = vec![2, 1];
        blocks[1].successors = vec![3];
        blocks[2].successors = vec![2, 3];
        let pcode = pcode_function(blocks);
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        builder
            .lower_block_stmts(&pcode.blocks[1])
            .expect("bypass lowering");
        let loop_body = builder
            .lower_block_stmts(&pcode.blocks[2])
            .expect("loop lowering");
        let carried_name = loop_body
            .iter()
            .filter_map(lhs_var)
            .last()
            .expect("loop-carried binding")
            .to_string();

        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 3,
            op_idx: 0,
        });
        let mut visiting = std::collections::HashSet::new();
        let resolved = builder
            .lower_varnode(&w20_be, &mut visiting)
            .expect("exit register lowering");

        assert_eq!(expr_var(&resolved), Some(carried_name.as_str()));
        assert!(
            !expr_contains_shr(&resolved),
            "AArch64 W-register projection must not read the high half of its X-register family: {resolved:?}"
        );
    }

    #[test]
    fn materialized_binding_invalidates_cached_terminators() {
        let rax = reg(0x00, 8);
        let blocks = vec![block_at(
            0x1000,
            0,
            vec![op(0, PcodeOpcode::Return, None, vec![])],
        )];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        builder
            .terminator_cache
            .insert(0, LoweredTerminator::Fallthrough(None));
        builder.ensure_temp_binding_for_output(
            &op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(0)]),
            &rax,
            false,
        );

        assert!(builder.terminator_cache.is_empty());
    }

    #[test]
    fn predicate_output_used_only_by_cbranch_is_not_materialized() {
        let rax = reg(0x00, 8);
        let pred = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: 0x3000,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::IntEqual,
                    Some(pred.clone()),
                    vec![rax.clone(), constant(0)],
                ),
                op(1, PcodeOpcode::CBranch, None, vec![constant(0x2000), pred]),
            ],
        )];
        blocks[0].successors = vec![1];
        blocks.push(block_at(
            0x2000,
            1,
            vec![op(2, PcodeOpcode::Return, None, vec![])],
        ));
        let pcode = pcode_function(blocks);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let body = builder
            .lower_block_stmts(&pcode.blocks[0])
            .expect("block statement lowering");

        assert!(
            body.is_empty(),
            "predicate-only output was materialized: {body:?}"
        );
    }

    #[test]
    fn loop_head_update_can_reach_backedge_tail() {
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1010), constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![op(1, PcodeOpcode::Branch, None, vec![constant(0x1000)])],
            ),
            block_at(0x1020, 2, vec![op(2, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1, 2];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(builder.block_can_reach(0, 1, 0));
    }

    #[test]
    fn nonlocal_use_scan_ignores_unreachable_preheader_use() {
        let rax = reg(0x00, 8);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntXor,
                        Some(rax.clone()),
                        vec![rax.clone(), rax.clone()],
                    ),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(rax.clone()),
                        vec![rax.clone(), constant(1)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![1];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert_eq!(
            builder.first_output_use_site_outside_block(0x1010, &rax),
            None
        );
        assert_eq!(
            builder.first_output_use_site_outside_block(0x1000, &rax),
            Some((0x1010, 0, 2))
        );
    }

    #[test]
    fn loop_carried_overwrite_provenance_marks_boolean_flag_without_multiequal() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(varnode(0x14)),
                        vec![output.clone()],
                    ),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntSCarry,
                        Some(output.clone()),
                        vec![varnode(0x12), varnode(0x13)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let detail = builder
            .describe_loop_carried_overwrite_provenance(&blocks[1], &output, &redef, 0x1000, 0)
            .expect("loop provenance");

        assert_eq!(detail.loop_header, 0x1000);
        assert_eq!(detail.backedge_block, 0x1010);
        assert!(!detail.has_multiequal);
        assert_eq!(detail.phi_input_count, 0);
        assert!(!detail.induction_like);
        assert_eq!(detail.carried_value_kind, LoopCarriedValueKind::BooleanFlag);
    }

    #[test]
    fn loop_carried_overwrite_provenance_marks_pointer_advance_with_multiequal() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::MultiEqual,
                        Some(varnode(0x50)),
                        vec![varnode(0x30), varnode(0x31)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(varnode(0x40)),
                        vec![output.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(output.clone()),
                        vec![output.clone(), constant(8)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let detail = builder
            .describe_loop_carried_overwrite_provenance(&blocks[1], &output, &redef, 0x1000, 1)
            .expect("loop provenance");

        assert_eq!(detail.loop_header, 0x1000);
        assert_eq!(detail.backedge_block, 0x1010);
        assert!(detail.has_multiequal);
        assert_eq!(detail.phi_input_count, 2);
        assert!(detail.induction_like);
        assert_eq!(
            detail.carried_value_kind,
            LoopCarriedValueKind::PointerAdvance
        );
    }

    #[test]
    fn loop_boolean_flag_proof_marks_same_guard_as_exit_for_header_predicate() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let predicate = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x21,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::BoolNegate,
                        Some(predicate.clone()),
                        vec![output.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1020), predicate.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x30), constant(0)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
            block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1, 2];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let proof = builder
            .describe_loop_boolean_flag_proof(&blocks[1], 0, &output, &redef, 0x1000, 0)
            .expect("loop boolean proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::BoolNegate);
        assert_eq!(proof.guard_family, LoopBooleanGuardFamily::NegatedFlag);
        assert!(proof.same_guard_as_exit);
        assert!(proof.consumer_is_loop_header_predicate);
        assert_eq!(proof.backedge_edge, Some(0x1010));
        assert_eq!(proof.exit_edge, Some(0x1020));
        assert!(!proof.old_def_has_pre_redef_use);
        assert!(proof.redef_dominates_backedge);
    }

    #[test]
    fn loop_boolean_flag_proof_marks_non_predicate_carried_state() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let copied = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x21,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let unrelated = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x22,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(copied.clone()),
                        vec![output.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1020), unrelated.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x30), constant(0)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
            block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1, 2];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let proof = builder
            .describe_loop_boolean_flag_proof(&blocks[1], 0, &output, &redef, 0x1000, 0)
            .expect("loop boolean proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::Copy);
        assert_eq!(proof.guard_family, LoopBooleanGuardFamily::NonPredicate);
        assert!(!proof.same_guard_as_exit);
        assert!(!proof.consumer_is_loop_header_predicate);
        assert_eq!(proof.backedge_edge, Some(0x1010));
        assert_eq!(proof.exit_edge, Some(0x1020));
        assert!(!proof.old_def_has_pre_redef_use);
        assert!(proof.redef_dominates_backedge);
    }

    #[test]
    fn loop_guard_refresh_dominance_proof_marks_multiple_backedge_blocks() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let predicate = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x21,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::BoolNegate,
                        Some(predicate.clone()),
                        vec![output.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1030), predicate.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x30), constant(0)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![op(4, PcodeOpcode::Branch, None, vec![constant(0x1000)])],
            ),
            block_at(0x1030, 3, vec![op(5, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1, 3];
        blocks[1].successors = vec![0];
        blocks[2].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let proof = builder
            .describe_loop_guard_refresh_dominance_proof(&blocks[1], 0, &output, &redef, 0x1000, 0)
            .expect("loop guard refresh dominance proof");

        assert!(proof.redef_before_backedge_branch);
        assert!(!proof.all_backedge_paths_covered);
        assert!(proof.header_predicate_uses_redef);
        assert_eq!(
            proof.reason,
            LoopGuardRefreshDominanceReason::MultipleBackedgeBlocks
        );
    }

    #[test]
    fn loop_guard_refresh_dominance_proof_marks_single_backedge_proved() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let predicate = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x21,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::BoolNegate,
                        Some(predicate.clone()),
                        vec![output.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1020), predicate.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x30), constant(0)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
            block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
        ];
        blocks[0].successors = vec![1, 2];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let proof = builder
            .describe_loop_guard_refresh_dominance_proof(&blocks[1], 0, &output, &redef, 0x1000, 0)
            .expect("loop guard refresh dominance proof");

        assert!(proof.redef_before_backedge_branch);
        assert!(proof.all_backedge_paths_covered);
        assert!(proof.header_predicate_uses_redef);
        assert_eq!(
            proof.reason,
            LoopGuardRefreshDominanceReason::ProvedBySingleBackedge
        );
    }
}
