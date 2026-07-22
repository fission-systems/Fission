use crate::midend::builder::PreviewBuilder;
use crate::midend::structuring::irreducible::NodeSplitResult;
use crate::midend::structuring::loop_analysis::LoopBody;
use crate::midend::support::StackSlot;
use fission_midend_dir::{DirBinding, DirStmt};
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

    pub(crate) fn params(&self) -> &BTreeMap<usize, DirBinding> {
        &self.builder.params
    }

    pub(crate) fn params_mut(&mut self) -> &mut BTreeMap<usize, DirBinding> {
        self.ir_version += 1;
        &mut self.builder.params
    }

    pub(crate) fn temps(&self) -> &BTreeMap<String, DirBinding> {
        &self.builder.temps
    }

    pub(crate) fn temps_mut(&mut self) -> &mut BTreeMap<String, DirBinding> {
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

    pub(crate) fn lowered_block_stmts(&self, block_idx: usize) -> Option<&[DirStmt]> {
        self.builder
            .lowered_block_stmts_cache
            .get(&block_idx)
            .map(|v| v.as_slice())
    }

    pub(crate) fn lowered_block_stmts_mut(&mut self, block_idx: usize) -> &mut Vec<DirStmt> {
        self.ir_version += 1;
        self.builder
            .lowered_block_stmts_cache
            .entry(block_idx)
            .or_insert_with(Vec::new)
    }

    pub(crate) fn set_lowered_block_stmts(&mut self, block_idx: usize, stmts: Vec<DirStmt>) {
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
        self.builder.successors = split.new_successors;
        self.builder.predecessors = split.new_predecessors;
        self.builder.virtual_block_map = split.virtual_to_original;
        self.builder.refresh_cfg_fact_cache();
        self.cfg_version += 1;
        self.ir_version += 1;
    }

    pub(crate) fn structured_body(&self) -> Option<&[DirStmt]> {
        self.builder.structured_body.as_deref()
    }

    pub(crate) fn set_structured_body(&mut self, body: Vec<DirStmt>) {
        self.ir_version += 1;
        self.builder.structured_body = Some(body);
    }
}
