use super::{apply_temp_rewrites, is_safe_intra_block_path, is_temp_varnode, trivial_source};
use crate::pcode::optimizer::DefUseTracker;
use crate::pcode::{PcodeFunction, PcodeOpcode, Varnode};

pub struct LoopHeaderTempCoalescer;

impl LoopHeaderTempCoalescer {
    pub fn new() -> Self {
        Self
    }

    pub fn eliminate(&self, func: &mut PcodeFunction, tracker: &DefUseTracker) -> bool {
        let mut rewrites = Vec::new();
        let mut queued_targets: Vec<Varnode> = Vec::new();

        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let Some(out) = &op.output else {
                    continue;
                };
                if !is_temp_varnode(out) {
                    continue;
                }
                if tracker.def_count(out) != 1 || tracker.has_overlapping_storage(out) {
                    continue;
                }

                let Some(source) = trivial_source(op) else {
                    continue;
                };
                let Some(phi_def) = tracker.get_def(&source) else {
                    continue;
                };
                if phi_def.block_idx != block_idx || phi_def.op_idx >= op_idx {
                    continue;
                }
                let phi_op = &func.blocks[phi_def.block_idx].ops[phi_def.op_idx];
                if phi_op.opcode != PcodeOpcode::MultiEqual {
                    continue;
                }
                if tracker.get_uses(out).len() != 1 {
                    continue;
                }
                let use_ref = tracker.get_uses(out)[0];
                if use_ref.block_idx != block_idx || use_ref.op_idx <= op_idx {
                    continue;
                }
                if queued_targets
                    .iter()
                    .any(|t| t.overlaps(&source) || t.overlaps(out))
                {
                    continue;
                }
                if !is_safe_intra_block_path(block, op_idx, use_ref.op_idx, out, &source) {
                    continue;
                }
                queued_targets.push(out.clone());
                rewrites.push((use_ref.block_idx, use_ref.op_idx, out.clone(), source));
            }
        }

        apply_temp_rewrites(func, rewrites)
    }
}
