//! Loop structuring surface for pcode.
//!
//! Free-function owners live in `fission-midend-structuring::loops` and are
//! dispatched via [`fission_midend_structuring::apply_collapse_rule`] (no
//! PreviewBuilder inherent thin wraps — ADR 0012 Phase D).
//!
//! Residual: pure p-code opcode scan for for-loop head discardability (cannot
//! move without a pcode dependency in midend-structuring).

use super::*;

pub use fission_midend_structuring::{
    lower_loop_body_subgraph, try_lower_dowhile, try_lower_for, try_lower_infloop,
    try_lower_infloop_with_break, try_lower_multiblock_dowhile, try_lower_multiblock_infloop,
    try_lower_while,
};

impl<'a> PreviewBuilder<'a> {
    /// P-code residual: head block ops are pure (no store/call/control) except
    /// a trailing CBranch terminator. Used by [`StructuringHost`] for-loop path.
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
