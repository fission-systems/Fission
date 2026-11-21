//! Individual P-code optimization passes.

mod copy_propagation;
mod cse;
mod dead_code;
pub mod def_use;
mod loop_header;
mod subvariable_flow;

use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use crate::space::is_unique_space_id;

pub(super) fn is_temp_varnode(vn: &Varnode) -> bool {
    !vn.is_constant && is_unique_space_id(vn.space_id)
}

pub(super) fn trivial_source(op: &PcodeOp) -> Option<Varnode> {
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast
            if op.inputs.len() == 1
                && op
                    .output
                    .as_ref()
                    .is_some_and(|out| out.size == op.inputs[0].size) =>
        {
            Some(op.inputs[0].clone())
        }
        _ => None,
    }
}

pub(super) fn is_safe_intra_block_path(
    block: &PcodeBasicBlock,
    from_op_idx: usize,
    to_op_idx: usize,
    target: &Varnode,
    source: &Varnode,
) -> bool {
    for op in &block.ops[(from_op_idx + 1)..to_op_idx] {
        if matches!(
            op.opcode,
            PcodeOpcode::Load
                | PcodeOpcode::Store
                | PcodeOpcode::Branch
                | PcodeOpcode::CBranch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Call
                | PcodeOpcode::CallInd
                | PcodeOpcode::CallOther
                | PcodeOpcode::Return
                | PcodeOpcode::Indirect
        ) {
            return false;
        }
        if op.inputs.iter().any(|input| input.overlaps(target)) {
            return false;
        }
        if op
            .output
            .as_ref()
            .is_some_and(|out| out.overlaps(source) || out.overlaps(target))
        {
            return false;
        }
    }
    true
}

pub(super) fn apply_temp_rewrites(
    func: &mut PcodeFunction,
    rewrites: Vec<(usize, usize, Varnode, Varnode)>,
) -> bool {
    let mut modified = false;
    for (block_idx, op_idx, target, source) in rewrites {
        let op = &mut func.blocks[block_idx].ops[op_idx];
        let mut changed = false;
        for input in &mut op.inputs {
            if *input == target {
                *input = source.clone();
                changed = true;
            }
        }
        modified |= changed;
    }
    modified
}

pub use copy_propagation::CopyPropagator;
pub use cse::CommonSubexpressionEliminator;
pub use dead_code::DeadCodeEliminator;
pub use def_use::DefUseTracker;
pub use loop_header::LoopHeaderTempCoalescer;
pub use subvariable_flow::SubvariableFlowOptimizer;
