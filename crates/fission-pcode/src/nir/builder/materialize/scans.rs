use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn collect_output_use_sites_in_block<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        let mut uses = Vec::new();
        for (idx, candidate) in block.ops.iter().enumerate().skip(op_idx + 1) {
            if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| VarnodeKey::from(input) == key)
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
                candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone())
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
                    .any(|input| VarnodeKey::from(input) == key)
            })
            .collect()
    }
}
