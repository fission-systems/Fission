use super::*;

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
            if candidate
                .output
                .as_ref()
                .is_some_and(|output| Self::varnode_matches_key(output, &key))
            {
                let is_false_redef = matches!(
                    candidate.opcode,
                    PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast | PcodeOpcode::Copy
                ) && candidate.inputs.first().is_some_and(|first_input| Self::varnode_matches_key(first_input, &key));

                if !is_false_redef {
                    break;
                }
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
            .find(|(_, candidate)| {
                let Some(candidate_output) = candidate.output.as_ref() else {
                    return false;
                };
                if !Self::varnode_matches_key(candidate_output, &key) {
                    return false;
                }
                if matches!(
                    candidate.opcode,
                    PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast | PcodeOpcode::Copy
                ) {
                    if let Some(first_input) = candidate.inputs.first() {
                        if Self::varnode_matches_key(first_input, &key) {
                            return false;
                        }
                    }
                }
                true
            })
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
            op(0, PcodeOpcode::IntAdd, Some(w20.clone()), vec![w20.clone(), constant(1)]),
            op(1, PcodeOpcode::IntZExt, Some(x20.clone()), vec![w20.clone()]),
        ]);

        let redef = PreviewBuilder::first_output_redefinition_in_block_from(&block, 1, &w20);
        assert!(redef.is_none(), "ZExt of the same register should be recognized as a false redefinition hazard and ignored");
    }
}
