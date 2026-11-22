//! Individual P-code optimization passes.

mod copy_propagation;
mod cse;
mod dead_code;
pub mod def_use;
mod loop_header;
mod subvariable_flow;

use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use crate::space::{
    REGISTER_SPACE_ID, RUST_SLEIGH_ALT_REGISTER_SPACE_ID, RUST_SLEIGH_REGISTER_SPACE_ID,
    RUST_SLEIGH_UNIQUE_SPACE_ID, UNIQUE_SPACE_ID,
};

/// The address space holding this function's p-code temporaries, or `None`
/// when the function shows no evidence of either numbering.
///
/// Two numberings are live (see [`crate::space`]). The Rust-Sleigh runtime
/// puts temporaries in space 2, registers in 4/5 and RAM in 3; the legacy
/// numbering puts registers in 1 and temporaries in 3. Space 3 is RAM in one
/// and unique in the other, so no single space id is "the" unique space: a
/// pass that treats a RAM varnode as a temporary forwards a global's value
/// past its store and then deletes the store. Any 2/4/5 varnode settles the
/// runtime numbering (the builder's `pcode_uses_runtime_space_numbering`
/// uses the same evidence); a register in space 1 settles the legacy one.
/// With neither, nothing is known to be a temporary.
pub(super) fn function_unique_space_id(func: &PcodeFunction) -> Option<u64> {
    let mut legacy = false;
    for op in func.blocks.iter().flat_map(|block| &block.ops) {
        for vn in op.output.iter().chain(op.inputs.iter()) {
            if vn.is_constant {
                continue;
            }
            match vn.space_id {
                RUST_SLEIGH_UNIQUE_SPACE_ID
                | RUST_SLEIGH_REGISTER_SPACE_ID
                | RUST_SLEIGH_ALT_REGISTER_SPACE_ID => return Some(RUST_SLEIGH_UNIQUE_SPACE_ID),
                REGISTER_SPACE_ID => legacy = true,
                _ => {}
            }
        }
    }
    legacy.then_some(UNIQUE_SPACE_ID)
}

/// Is `vn` a temporary of a function whose unique space is `unique_space_id`
/// (from [`function_unique_space_id`])?
pub(super) fn is_temp_varnode(vn: &Varnode, unique_space_id: u64) -> bool {
    !vn.is_constant && vn.space_id == unique_space_id
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
