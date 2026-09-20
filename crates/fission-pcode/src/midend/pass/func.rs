use crate::midend::builder::PreviewBuilder;
use crate::midend::structuring::irreducible::NodeSplitResult;
use crate::midend::structuring::loop_analysis::LoopBody;
use crate::midend::support::StackSlot;
use fission_midend_prehir::{PreHirBinding, PreHirStmt};
use std::collections::BTreeMap;

pub(crate) struct NirFunc<'a, 'b> {
    pub(crate) builder: &'a mut PreviewBuilder<'b>,
    cfg_version: usize,
    ir_version: usize,
}

impl<'a, 'b> NirFunc<'a, 'b> {
    pub(crate) fn new(builder: &'a mut PreviewBuilder<'b>) -> Self {
        Self {
            builder,
            cfg_version: 0,
            ir_version: 0,
        }
    }

    pub(crate) fn cfg_version(&self) -> usize {
        self.cfg_version
    }

    pub(crate) fn ir_version(&self) -> usize {
        self.ir_version
    }

    pub(crate) fn successors(&self) -> &[Vec<usize>] {
        &self.builder.successors
    }

    pub(crate) fn successors_mut(&mut self) -> &mut Vec<Vec<usize>> {
        self.cfg_version += 1;
        self.ir_version += 1;
        &mut self.builder.successors
    }

    pub(crate) fn predecessors(&self) -> &[Vec<usize>] {
        &self.builder.predecessors
    }

    pub(crate) fn predecessors_mut(&mut self) -> &mut Vec<Vec<usize>> {
        self.cfg_version += 1;
        self.ir_version += 1;
        &mut self.builder.predecessors
    }

    pub(crate) fn block_count(&self) -> usize {
        self.builder.pcode.blocks.len() + self.builder.virtual_block_map.len()
    }

    pub(crate) fn virtual_block_map(&self) -> &[usize] {
        &self.builder.virtual_block_map
    }

    pub(crate) fn virtual_block_map_mut(&mut self) -> &mut Vec<usize> {
        self.cfg_version += 1;
        self.ir_version += 1;
        &mut self.builder.virtual_block_map
    }

    pub(crate) fn locals(&self) -> &BTreeMap<i64, StackSlot> {
        &self.builder.locals
    }

    pub(crate) fn locals_mut(&mut self) -> &mut BTreeMap<i64, StackSlot> {
        self.ir_version += 1;
        &mut self.builder.locals
    }

    pub(crate) fn params(&self) -> &BTreeMap<usize, PreHirBinding> {
        &self.builder.params
    }

    pub(crate) fn params_mut(&mut self) -> &mut BTreeMap<usize, PreHirBinding> {
        self.ir_version += 1;
        &mut self.builder.params
    }

    pub(crate) fn temps(&self) -> &BTreeMap<String, PreHirBinding> {
        &self.builder.temps
    }

    pub(crate) fn temps_mut(&mut self) -> &mut BTreeMap<String, PreHirBinding> {
        self.ir_version += 1;
        &mut self.builder.temps
    }

    pub(crate) fn loop_bodies(&self) -> &[LoopBody] {
        &self.builder.loop_bodies
    }

    pub(crate) fn loop_bodies_mut(&mut self) -> &mut Vec<LoopBody> {
        self.ir_version += 1;
        &mut self.builder.loop_bodies
    }

    pub(crate) fn lowered_block_stmts(&self, block_idx: usize) -> Option<&[PreHirStmt]> {
        self.builder
            .lowered_block_stmts_cache
            .get(&block_idx)
            .map(|v| v.as_slice())
    }

    pub(crate) fn lowered_block_stmts_mut(&mut self, block_idx: usize) -> &mut Vec<PreHirStmt> {
        self.ir_version += 1;
        self.builder
            .lowered_block_stmts_cache
            .entry(block_idx)
            .or_insert_with(Vec::new)
    }

    pub(crate) fn set_lowered_block_stmts(&mut self, block_idx: usize, stmts: Vec<PreHirStmt>) {
        self.ir_version += 1;
        self.builder
            .lowered_block_stmts_cache
            .insert(block_idx, stmts);
    }

    pub(crate) fn apply_virtual_goto_edge(&mut self, from: usize, to: usize) -> bool {
        if self.builder.apply_virtual_goto_edge(from, to) {
            self.cfg_version += 1;
            self.ir_version += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn apply_node_splits(&mut self, split: NodeSplitResult) {
        self.builder
            .extend_virtual_block_target_keys(&split.virtual_to_original);
        self.builder.successors = split.new_successors;
        self.builder.predecessors = split.new_predecessors;
        self.builder.virtual_block_map = split.virtual_to_original;
        self.builder.refresh_cfg_fact_cache();
        self.cfg_version += 1;
        self.ir_version += 1;
    }

    pub(crate) fn structured_body(&self) -> Option<&[PreHirStmt]> {
        self.builder.structured_body.as_deref()
    }

    pub(crate) fn set_structured_body(&mut self, body: Vec<PreHirStmt>) {
        self.ir_version += 1;
        self.builder.structured_body = Some(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::ir::MlilPreviewOptions;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_structuring::StructuringHost;
    use fission_midend_structuring::linear_types::LoweredTerminator;

    fn branch_function() -> PcodeFunction {
        PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1000,
                        output: None,
                        inputs: vec![Varnode::constant(0x1010, 8)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: Vec::new(),
                    ops: vec![PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x1010,
                        output: None,
                        inputs: Vec::new(),
                        asm_mnemonic: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn node_split_clones_get_distinct_target_keys() {
        let pcode = branch_function();
        let options = MlilPreviewOptions::default();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let mut ir = NirFunc::new(&mut builder);
        ir.apply_node_splits(NodeSplitResult {
            new_successors: vec![vec![2], vec![], vec![]],
            new_predecessors: vec![vec![], vec![], vec![0]],
            virtual_to_original: vec![1],
            original_count: 2,
            splits_applied: 1,
        });

        assert_ne!(builder.block_target_key(1), builder.block_target_key(2));
        assert_eq!(
            builder.find_block_index_by_address(builder.block_target_key(2)),
            Some(2)
        );
    }

    #[test]
    fn branch_lowering_follows_a_redirected_node_split_edge() {
        let pcode = branch_function();
        let options = MlilPreviewOptions::default();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let mut ir = NirFunc::new(&mut builder);
        ir.apply_node_splits(NodeSplitResult {
            new_successors: vec![vec![2], vec![], vec![]],
            new_predecessors: vec![vec![], vec![], vec![0]],
            virtual_to_original: vec![1],
            original_count: 2,
            splits_applied: 1,
        });
        drop(ir);

        let lowered = builder
            .lower_block_terminator(0)
            .expect("redirected branch should lower");
        assert_eq!(
            lowered,
            LoweredTerminator::Goto(builder.block_target_key(2))
        );
    }
}
