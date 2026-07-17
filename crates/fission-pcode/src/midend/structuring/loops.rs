//! Loop structuring — thin wrappers over free functions in
//! `fission-midend-structuring::loops`.

use super::*;

pub use fission_midend_structuring::{
    lower_loop_body_subgraph, try_lower_dowhile, try_lower_for, try_lower_infloop,
    try_lower_infloop_with_break, try_lower_multiblock_dowhile, try_lower_multiblock_infloop,
    try_lower_while,
};

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn get_loop_body(
        &self,
        head_idx: usize,
    ) -> Option<&crate::midend::structuring::loop_analysis::LoopBody> {
        self.loop_bodies.iter().find(|lb| lb.head == head_idx)
    }

    pub(super) fn try_lower_infloop_with_break(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_infloop_with_break(self, idx)
    }

    pub(super) fn try_lower_infloop(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_infloop(self, idx)
    }

    pub(super) fn try_lower_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_dowhile(self, idx)
    }

    pub(super) fn try_lower_while(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_while(self, idx)
    }

    pub(super) fn try_lower_multiblock_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_multiblock_dowhile(self, idx)
    }

    pub(super) fn try_lower_for(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_for(self, idx)
    }

    pub(super) fn try_lower_multiblock_infloop(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_multiblock_infloop(self, idx)
    }

    pub(super) fn lower_loop_body_subgraph(
        &mut self,
        body_set: &HashSet<usize>,
        start_idx: usize,
        break_idx: Option<usize>,
        head_idx: usize,
    ) -> Result<Option<Vec<HirStmt>>, MlilPreviewError> {
        lower_loop_body_subgraph(self, body_set, start_idx, break_idx, head_idx)
    }

    pub(crate) fn for_condition_head_has_only_discardable_pure_ops(
        block: &crate::pcode::PcodeBasicBlock,
    ) -> bool {
        let terminator_idx = block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        });
        block.ops.iter().enumerate().all(|(op_idx, op)| {
            if Some(op_idx) == terminator_idx {
                return op.opcode == PcodeOpcode::CBranch;
            }
            !matches!(
                op.opcode,
                PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }
}
