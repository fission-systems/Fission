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
    pub(in crate::midend::builder) fn varnode_matches_key(
        varnode: &Varnode,
        key: &VarnodeKey,
    ) -> bool {
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

    /// Whether the op at `idx` sits strictly inside an instruction-local skip:
    /// a `Branch`/`CBranch` earlier in the block whose target is a later op of
    /// the same block. A write there happens only on the path that did not take
    /// the skip, so it does not end the lifetime of an earlier definition of
    /// the same storage: on the skipped path that earlier value is still the
    /// one a later read sees.
    ///
    /// SLEIGH builds a conditional select this way (AArch64 `csel` is
    /// `tmp = a; if (cond) goto L; tmp = b; L:`), and x86 `cmov` is the same
    /// shape without the first write.
    pub(in crate::midend::builder) fn op_is_inside_instruction_local_skip(
        block: &crate::pcode::PcodeBasicBlock,
        idx: usize,
    ) -> bool {
        block
            .ops
            .iter()
            .enumerate()
            .take(idx)
            .any(|(branch_idx, op)| {
                matches!(op.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
                    && op.inputs.first().is_some_and(|target| {
                        crate::midend::cfg::same_block_forward_branch_target_op_idx(
                            block,
                            branch_idx,
                            block.ops.len(),
                            op,
                            target,
                        )
                        .is_some_and(|target_idx| target_idx > idx)
                    })
            })
    }

    /// Whether the definition at `op_idx` is shadowed by a write to the same
    /// storage that sits inside a later instruction-local skip, with no
    /// unconditional write to it in between. Then the definition still reaches
    /// whatever follows the skip on the path that took it.
    pub(in crate::midend::builder) fn is_overwritten_only_inside_instruction_local_skip(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let mut guarded_write = false;
        for (idx, candidate) in block.ops.iter().enumerate().skip(op_idx + 1) {
            if !Self::op_kills_varnode_definition(candidate, &key) {
                continue;
            }
            if Self::op_is_inside_instruction_local_skip(block, idx) {
                guarded_write = true;
            } else {
                return guarded_write;
            }
        }
        guarded_write
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

    pub(in crate::midend::builder) fn op_kills_varnode_definition(
        op: &PcodeOp,
        key: &VarnodeKey,
    ) -> bool {
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

    pub(in crate::midend::builder) fn op_clobbers_return_register_definition(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        op.output.is_none()
            && matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
            && self.register_namer().is_primary_return_register(output)
            && !self.call_is_return_target_artifact(block, op_idx)
            && !self.call_is_terminal_branchind_artifact(block, op_idx)
    }

    /// Find the first use reached after leaving the definition's block.
    ///
    /// The search follows CFG edges from the exact definition site and stops a
    /// path at the first overlapping write. Same-block uses after the
    /// definition are intentionally ignored here; callers analyze those with
    /// `collect_output_use_sites_in_block`. Re-entering the block over a
    /// backedge is a non-local use and is therefore included.
    pub(in crate::midend::builder) fn first_reaching_output_use_after_block_exit(
        &self,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, u64, usize, u32)> {
        let key = VarnodeKey::from(output);
        let definition_block = self.pcode.blocks.get(definition_block_idx)?;
        // Only ops that can hold a varnode matching `key` can read or redefine
        // it, so each block is judged from its candidates alone. An output-less
        // call also redefines a return register without naming it, so for one
        // the calls join the candidates.
        let candidates = if self.register_namer().is_primary_return_register(output) {
            self.space_op_index.candidate_ops_with_calls(&key)
        } else {
            self.space_op_index.candidate_ops(&key)
        };

        if SpaceOpIndex::candidates_in_block(&candidates, definition_block_idx)
            .iter()
            .any(|&(_, idx)| {
                idx > definition_op_idx
                    && (Self::op_kills_varnode_definition(&definition_block.ops[idx], &key)
                        || self.op_clobbers_return_register_definition(
                            definition_block,
                            idx,
                            output,
                        ))
            })
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
            for &(_, op_idx) in SpaceOpIndex::candidates_in_block(&candidates, block_idx) {
                let op = &block.ops[op_idx];
                if op
                    .inputs
                    .iter()
                    .any(|input| Self::varnode_matches_key(input, &key))
                {
                    return Some((block_idx, block.start_address, op_idx, op.seq_num));
                }
                if Self::op_kills_varnode_definition(op, &key)
                    || self.op_clobbers_return_register_definition(block, op_idx, output)
                {
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
        if ((definition_op_idx + 1)..definition_block.ops.len()).any(|idx| {
            Self::op_kills_varnode_definition(&definition_block.ops[idx], &key)
                || self.op_clobbers_return_register_definition(definition_block, idx, output)
        }) {
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
            if (0..block.ops.len()).any(|op_idx| {
                Self::op_kills_varnode_definition(&block.ops[op_idx], &key)
                    || self.op_clobbers_return_register_definition(block, op_idx, output)
            }) {
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

    /// The scan the candidate index replaced: every op of every reachable block.
    fn reference_first_reaching_use(
        builder: &PreviewBuilder,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, u64, usize, u32)> {
        let key = VarnodeKey::from(output);
        let definition_block = builder.pcode.blocks.get(definition_block_idx)?;
        if ((definition_op_idx + 1)..definition_block.ops.len()).any(|idx| {
            PreviewBuilder::op_kills_varnode_definition(&definition_block.ops[idx], &key)
                || builder.op_clobbers_return_register_definition(definition_block, idx, output)
        }) {
            return None;
        }
        let mut queue = VecDeque::new();
        queue.extend(
            builder
                .successors
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
            let block = builder.pcode.blocks.get(block_idx)?;
            let mut killed = false;
            for (op_idx, op) in block.ops.iter().enumerate() {
                if op
                    .inputs
                    .iter()
                    .any(|input| PreviewBuilder::varnode_matches_key(input, &key))
                {
                    return Some((block_idx, block.start_address, op_idx, op.seq_num));
                }
                if PreviewBuilder::op_kills_varnode_definition(op, &key)
                    || builder.op_clobbers_return_register_definition(block, op_idx, output)
                {
                    killed = true;
                    break;
                }
            }
            if !killed {
                queue.extend(
                    builder
                        .successors
                        .get(block_idx)
                        .into_iter()
                        .flatten()
                        .copied(),
                );
            }
        }
        None
    }

    #[test]
    fn an_output_less_call_ends_a_return_register_definition_but_not_other_registers() {
        let rax = reg(0x0, 8);
        let rcx = reg(0x8, 8);
        let used = varnode(0x100);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
                    op(1, PcodeOpcode::Copy, Some(rcx.clone()), vec![constant(2)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(2, PcodeOpcode::Call, None, vec![constant(0x9000)])],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(3, PcodeOpcode::Copy, Some(used.clone()), vec![rax.clone()]),
                    op(4, PcodeOpcode::Copy, Some(used), vec![rcx.clone()]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert_eq!(
            builder.first_reaching_output_use_after_block_exit(0, 0, &rax),
            None,
            "the call clobbers the return register before the read"
        );
        assert_eq!(
            builder.first_reaching_output_use_after_block_exit(0, 1, &rcx),
            Some((2, 0x1020, 1, 4)),
            "a call does not redefine another register"
        );
    }

    #[test]
    fn first_reaching_use_matches_exhaustive_scan() {
        let rax = reg(0x0, 8);
        let eax = reg(0x0, 4);
        let ah = reg(0x1, 1);
        let rcx = reg(0x8, 8);
        let flag = reg(0x200, 1);
        let straddle = reg(0x1e, 4);
        let wide = reg(0x100, 4096);
        let tmp = varnode(0x100);
        let tmp2 = varnode(0x108);
        let zero = reg(0x30, 0);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(flag.clone()),
                        vec![rax.clone(), constant(0)],
                    ),
                    op(2, PcodeOpcode::Copy, Some(tmp.clone()), vec![rcx.clone()]),
                    op(
                        3,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1010), flag.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntAdd,
                        Some(rcx.clone()),
                        vec![eax.clone(), tmp.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(straddle.clone()),
                        vec![zero.clone()],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntZExt,
                        Some(rax.clone()),
                        vec![eax.clone()],
                    ),
                    op(1, PcodeOpcode::Copy, Some(ah.clone()), vec![constant(2)]),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(wide.clone()),
                        vec![straddle.clone()],
                    ),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![
                    op(0, PcodeOpcode::Copy, Some(tmp2.clone()), vec![rax.clone()]),
                    op(
                        1,
                        PcodeOpcode::IntAdd,
                        Some(rcx.clone()),
                        vec![rcx.clone(), tmp2.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1010), flag.clone()],
                    ),
                ],
            ),
            block_at(
                0x1040,
                4,
                vec![
                    op(0, PcodeOpcode::Copy, Some(zero.clone()), vec![wide.clone()]),
                    op(2, PcodeOpcode::Call, None, vec![constant(0x9000)]),
                    op(1, PcodeOpcode::Return, None, vec![rax.clone()]),
                ],
            ),
        ];
        blocks[0].successors = vec![1, 2];
        blocks[1].successors = vec![3];
        blocks[2].successors = vec![3];
        blocks[3].successors = vec![1, 4];
        // a return-register definition whose only reachable read is behind an
        // output-less call
        blocks.push(block_at(
            0x1050,
            5,
            vec![op(
                0,
                PcodeOpcode::Copy,
                Some(rax.clone()),
                vec![constant(7)],
            )],
        ));
        blocks.push(block_at(
            0x1060,
            6,
            vec![
                op(0, PcodeOpcode::Call, None, vec![constant(0x9000)]),
                op(1, PcodeOpcode::Copy, Some(tmp.clone()), vec![rax.clone()]),
            ],
        ));
        blocks[5].successors = vec![6];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let mut found = 0;
        for (block_idx, block) in blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                let expected = reference_first_reaching_use(&builder, block_idx, op_idx, output);
                found += usize::from(expected.is_some());
                assert_eq!(
                    builder.first_reaching_output_use_after_block_exit(block_idx, op_idx, output),
                    expected,
                    "definition at block {block_idx} op {op_idx}"
                );
            }
        }
        assert!(found > 0, "fixture must exercise real reaching uses");
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
