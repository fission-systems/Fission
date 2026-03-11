use crate::pcode::optimizer::DefUseTracker;
use crate::pcode::{PcodeFunction, PcodeOpcode, Varnode};

pub struct CopyPropagator;

impl CopyPropagator {
    pub fn new() -> Self {
        Self
    }

    pub fn eliminate(&self, func: &mut PcodeFunction, tracker: &DefUseTracker) -> bool {
        let mut rewrites = Vec::new();

        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let Some(out) = &op.output else {
                    continue;
                };
                if !self.is_temp_varnode(out) {
                    continue;
                }

                let Some(source) = self.trivial_source(op) else {
                    continue;
                };
                if tracker.get_uses(out).len() != 1 {
                    continue;
                }
                let use_ref = tracker.get_uses(out)[0];
                if use_ref.block_idx != block_idx || use_ref.op_idx <= op_idx {
                    continue;
                }
                if tracker
                    .get_def(&source)
                    .and_then(|def_ref| func.blocks.get(def_ref.block_idx)?.ops.get(def_ref.op_idx))
                    .is_some_and(|def_op| def_op.opcode == PcodeOpcode::MultiEqual)
                {
                    continue;
                }
                if !self.is_safe_path(block, op_idx, use_ref.op_idx, out, &source) {
                    continue;
                }
                rewrites.push((use_ref.block_idx, use_ref.op_idx, out.clone(), source));
            }
        }

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

    fn trivial_source(&self, op: &crate::pcode::PcodeOp) -> Option<Varnode> {
        match op.opcode {
            PcodeOpcode::Copy if op.inputs.len() == 1 => Some(op.inputs[0].clone()),
            PcodeOpcode::Cast
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

    fn is_temp_varnode(&self, vn: &Varnode) -> bool {
        !vn.is_constant && vn.space_id == 1
    }

    fn is_safe_path(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        from_op_idx: usize,
        to_op_idx: usize,
        target: &Varnode,
        source: &Varnode,
    ) -> bool {
        for op in &block.ops[(from_op_idx + 1)..to_op_idx] {
            if self.is_boundary(op.opcode) {
                return false;
            }
            if op.inputs.iter().any(|input| input == target) {
                return false;
            }
            if op
                .output
                .as_ref()
                .is_some_and(|out| out == source || out == target)
            {
                return false;
            }
        }
        true
    }

    fn is_boundary(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
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
        )
    }
}
