use super::*;
use std::collections::VecDeque;

/// Opaque evidence that one exact p-code definition reaches a target block
/// entry without an overlapping write on the selected CFG path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DefinitionReachesBlockProof {
    definition_block: usize,
    definition_op: usize,
    target_block: usize,
}

impl DefinitionReachesBlockProof {
    pub(super) fn definition_site(self) -> (usize, usize) {
        (self.definition_block, self.definition_op)
    }

    pub(super) fn target_block(self) -> usize {
        self.target_block
    }
}

/// Opaque evidence that one exact p-code definition remains live until a
/// machine return terminator on at least one CFG path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DefinitionReachesReturnProof {
    definition_block: usize,
    definition_op: usize,
    return_block: usize,
}

impl DefinitionReachesReturnProof {
    pub(super) fn definition_site(self) -> (usize, usize) {
        (self.definition_block, self.definition_op)
    }

    pub(super) fn return_block(self) -> usize {
        self.return_block
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn varnode_matches_key(varnode: &Varnode, key: &VarnodeKey) -> bool {
        let candidate = VarnodeKey::from(varnode);
        if candidate == *key {
            return true;
        }
        if candidate.is_constant
            || key.is_constant
            || !is_register_space_id(candidate.space_id)
            || !is_register_space_id(key.space_id)
            || candidate.space_id != key.space_id
        {
            return false;
        }

        let candidate_end = candidate.offset.saturating_add(u64::from(candidate.size));
        let key_end = key.offset.saturating_add(u64::from(key.size));
        candidate.offset < key_end && key.offset < candidate_end
    }

    pub(super) fn collect_output_use_sites_in_block<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        let mut uses = Vec::new();
        for (idx, candidate) in block.ops.iter().enumerate().skip(op_idx + 1) {
            if Self::op_kills_varnode_definition(candidate, &key) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| Self::varnode_matches_key(input, &key))
            {
                uses.push((idx, candidate));
            }
        }
        uses
    }

    pub(super) fn first_output_redefinition_in_block<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        Self::first_output_redefinition_in_block_from(block, op_idx + 1, output)
    }

    pub(super) fn first_output_redefinition_in_block_from<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        block
            .ops
            .iter()
            .enumerate()
            .skip(start_idx)
            .find(|(_, candidate)| Self::op_kills_varnode_definition(candidate, &key))
    }

    pub(super) fn op_kills_varnode_definition(op: &PcodeOp, key: &VarnodeKey) -> bool {
        let Some(output) = op.output.as_ref() else {
            return false;
        };
        if !Self::varnode_matches_key(output, key) {
            return false;
        }
        let preserves_value = matches!(
            op.opcode,
            PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast | PcodeOpcode::Copy
        ) && op
            .inputs
            .first()
            .is_some_and(|input| Self::varnode_matches_key(input, key));
        !preserves_value
    }

    /// Find the first use reached after leaving the definition's block.
    ///
    /// The search follows CFG edges from the exact definition site and stops a
    /// path at the first overlapping write. Same-block uses after the
    /// definition are intentionally ignored here; callers analyze those with
    /// `collect_output_use_sites_in_block`. Re-entering the block over a
    /// backedge is a non-local use and is therefore included.
    pub(super) fn first_reaching_output_use_after_block_exit(
        &self,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, u64, usize, u32)> {
        let key = VarnodeKey::from(output);
        let definition_block = self.pcode.blocks.get(definition_block_idx)?;

        if definition_block
            .ops
            .iter()
            .skip(definition_op_idx + 1)
            .any(|op| Self::op_kills_varnode_definition(op, &key))
        {
            return None;
        }

        let mut queue = VecDeque::new();
        queue.extend(
            self.successors
                .get(definition_block_idx)
                .into_iter()
                .flatten()
                .copied(),
        );
        let mut visited = HashSet::default();

        while let Some(block_idx) = queue.pop_front() {
            if !visited.insert(block_idx) {
                continue;
            }
            let block = self.pcode.blocks.get(block_idx)?;
            let mut killed = false;
            for (op_idx, op) in block.ops.iter().enumerate() {
                if op
                    .inputs
                    .iter()
                    .any(|input| Self::varnode_matches_key(input, &key))
                {
                    return Some((block_idx, block.start_address, op_idx, op.seq_num));
                }
                if Self::op_kills_varnode_definition(op, &key) {
                    killed = true;
                    break;
                }
            }
            if !killed {
                queue.extend(
                    self.successors
                        .get(block_idx)
                        .into_iter()
                        .flatten()
                        .copied(),
                );
            }
        }
        None
    }

    pub(super) fn prove_definition_reaches_block_entry(
        &self,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output: &Varnode,
        target_block_idx: usize,
    ) -> Option<DefinitionReachesBlockProof> {
        let key = VarnodeKey::from(output);
        let definition_block = self.pcode.blocks.get(definition_block_idx)?;
        if definition_block
            .ops
            .iter()
            .skip(definition_op_idx + 1)
            .any(|op| Self::op_kills_varnode_definition(op, &key))
        {
            return None;
        }

        let mut queue = VecDeque::new();
        queue.extend(
            self.successors
                .get(definition_block_idx)
                .into_iter()
                .flatten()
                .copied(),
        );
        let mut visited = HashSet::default();
        while let Some(block_idx) = queue.pop_front() {
            if block_idx == target_block_idx {
                return Some(DefinitionReachesBlockProof {
                    definition_block: definition_block_idx,
                    definition_op: definition_op_idx,
                    target_block: target_block_idx,
                });
            }
            if !visited.insert(block_idx) {
                continue;
            }
            let block = self.pcode.blocks.get(block_idx)?;
            if block
                .ops
                .iter()
                .any(|op| Self::op_kills_varnode_definition(op, &key))
            {
                continue;
            }
            queue.extend(
                self.successors
                    .get(block_idx)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        None
    }

    pub(super) fn prove_definition_reaches_return(
        &self,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output: &Varnode,
    ) -> Option<DefinitionReachesReturnProof> {
        let key = VarnodeKey::from(output);
        let mut queue = VecDeque::from([(definition_block_idx, definition_op_idx + 1)]);
        let mut visited = HashSet::default();

        while let Some((block_idx, start_op_idx)) = queue.pop_front() {
            if !visited.insert((block_idx, start_op_idx)) {
                continue;
            }
            let block = self.pcode.blocks.get(block_idx)?;
            let mut killed = false;
            for op in block.ops.iter().skip(start_op_idx) {
                if op.opcode == PcodeOpcode::Return {
                    return Some(DefinitionReachesReturnProof {
                        definition_block: definition_block_idx,
                        definition_op: definition_op_idx,
                        return_block: block_idx,
                    });
                }
                if matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                ) || Self::op_kills_varnode_definition(op, &key)
                {
                    killed = true;
                    break;
                }
            }
            if killed {
                continue;
            }
            queue.extend(
                self.successors
                    .get(block_idx)
                    .into_iter()
                    .flatten()
                    .copied()
                    .map(|successor| (successor, 0)),
            );
        }
        None
    }

    pub(super) fn collect_output_use_sites_in_block_unbounded<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter(|(_, candidate)| {
                candidate
                    .inputs
                    .iter()
                    .any(|input| Self::varnode_matches_key(input, &key))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    #[test]
    fn materialize_scans_match_overlapping_register_aliases() {
        let x20 = reg(0x40a0, 8);
        let w20 = reg(0x40a0, 4);
        let x21 = reg(0x40a8, 8);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(x20.clone()), vec![constant(0)]),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(x21.clone()),
                vec![w20.clone(), constant(1)],
            ),
            op(2, PcodeOpcode::Copy, Some(w20.clone()), vec![constant(2)]),
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(x21.clone()),
                vec![x20.clone(), constant(3)],
            ),
        ]);

        let bounded_uses = PreviewBuilder::collect_output_use_sites_in_block(&block, 0, &x20);
        assert_eq!(bounded_uses.len(), 1);
        assert_eq!(bounded_uses[0].0, 1);

        let unbounded_uses =
            PreviewBuilder::collect_output_use_sites_in_block_unbounded(&block, 0, &x20);
        assert_eq!(
            unbounded_uses
                .iter()
                .map(|(idx, _)| *idx)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );

        let redef = PreviewBuilder::first_output_redefinition_in_block_from(&block, 1, &x20)
            .expect("w20 should redefine the overlapping x20 range");
        assert_eq!(redef.0, 2);
    }

    #[test]
    fn materialize_scans_ignores_false_redefinition_hazards() {
        let x20 = reg(0x40a0, 8);
        let w20 = reg(0x40a0, 4);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(w20.clone()),
                vec![w20.clone(), constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntZExt,
                Some(x20.clone()),
                vec![w20.clone()],
            ),
        ]);

        let redef = PreviewBuilder::first_output_redefinition_in_block_from(&block, 1, &w20);
        assert!(
            redef.is_none(),
            "ZExt of the same register should be recognized as a false redefinition hazard and ignored"
        );
    }

    #[test]
    fn cross_block_use_requires_a_kill_free_definition_path() {
        let value = reg(0x8, 4);
        let used = varnode(0x100);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(value.clone()), vec![constant(1)]),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(2, PcodeOpcode::Copy, Some(value.clone()), vec![constant(2)]),
                    op(3, PcodeOpcode::Copy, Some(used), vec![value.clone()]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert_eq!(
            builder.first_output_use_site_outside_block_by_index(0, 0, &value),
            None,
            "a successor definition must kill the predecessor value before the later use"
        );
    }

    #[test]
    fn backedge_use_requires_the_definition_to_survive_the_latch() {
        let value = reg(0x8, 4);
        let used = varnode(0x100);
        let mut blocks = vec![block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(used), vec![value.clone()]),
                op(1, PcodeOpcode::Copy, Some(value.clone()), vec![constant(1)]),
                op(2, PcodeOpcode::Copy, Some(value.clone()), vec![constant(2)]),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
            ],
        )];
        blocks[0].successors = vec![0];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert_eq!(
            builder.first_output_use_site_outside_block_by_index(0, 1, &value),
            None,
            "the earlier definition is killed before the backedge"
        );
        assert!(
            builder
                .prove_definition_reaches_block_entry(0, 1, &value, 0)
                .is_none(),
            "a killed phase must not receive an explicit loop-header binding"
        );
        assert_eq!(
            builder
                .first_output_use_site_outside_block_by_index(0, 2, &value)
                .map(|(_, _, op_idx, _)| op_idx),
            Some(0),
            "the latch-reaching definition must remain visible on the next iteration"
        );
        let proof = builder
            .prove_definition_reaches_block_entry(0, 2, &value, 0)
            .expect("the final definition reaches the self-loop header");
        assert_eq!(proof.definition_site(), (0, 2));
        assert_eq!(proof.target_block(), 0);
    }
}
