use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
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
            if idx == stop_idx {
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
