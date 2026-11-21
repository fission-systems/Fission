use super::def_use::DefUseTracker;
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode, truncate_to_varnode_width};
use crate::space::{
    RUST_SLEIGH_ALT_REGISTER_SPACE_ID, RUST_SLEIGH_REGISTER_SPACE_ID, RUST_SLEIGH_UNIQUE_SPACE_ID,
    UNIQUE_SPACE_ID,
};

pub struct SubvariableFlowOptimizer;

fn infer_unique_space_id(func: &PcodeFunction) -> u64 {
    for block in &func.blocks {
        for op in &block.ops {
            for vn in op.output.iter().chain(op.inputs.iter()) {
                if !vn.is_constant
                    && matches!(
                        vn.space_id,
                        RUST_SLEIGH_UNIQUE_SPACE_ID
                            | RUST_SLEIGH_REGISTER_SPACE_ID
                            | RUST_SLEIGH_ALT_REGISTER_SPACE_ID
                    )
                {
                    return RUST_SLEIGH_UNIQUE_SPACE_ID;
                }
            }
        }
    }
    UNIQUE_SPACE_ID
}

fn initial_next_temp_offset(func: &PcodeFunction, unique_space_id: u64) -> u64 {
    let mut max_end: u64 = 0x10000;
    for block in &func.blocks {
        for op in &block.ops {
            for vn in op.output.iter().chain(op.inputs.iter()) {
                if !vn.is_constant && vn.space_id == unique_space_id {
                    let end = vn.offset.saturating_add(u64::from(vn.size.max(1)));
                    max_end = max_end.max(end);
                }
            }
        }
    }
    max_end.next_multiple_of(8)
}

fn alloc_temp(next_offset: &mut u64, unique_space_id: u64, size: u32) -> Varnode {
    let offset = *next_offset;
    let step = u64::from(size.max(1)).next_multiple_of(8);
    *next_offset = offset.saturating_add(step);
    Varnode {
        space_id: unique_space_id,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

impl SubvariableFlowOptimizer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn eliminate(&mut self, func: &mut PcodeFunction, def_use: &DefUseTracker) -> bool {
        let mut modified = false;

        // Collect operations to narrow: (block_idx, op_idx, target_size)
        let mut optimizations = Vec::new();

        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let Some(out) = op.output.as_ref() else {
                    continue;
                };
                let orig_size = out.size;

                if orig_size <= 1 {
                    continue;
                }

                // Get exactly what bits are consumed.
                let consume_mask = def_use.get_consume_mask(out);

                // If nothing is consumed, let DCE remove it.
                if consume_mask == 0 {
                    continue;
                }

                let target_size = if consume_mask <= 0xFF {
                    1
                } else if consume_mask <= 0xFFFF {
                    2
                } else if consume_mask <= 0xFFFFFFFF && orig_size > 4 {
                    4
                } else {
                    orig_size // No viable narrowing, keep original size
                };

                if target_size < orig_size && Self::can_narrow_opcode(op.opcode) {
                    optimizations.push((block_idx, op_idx, target_size));
                }
            }
        }

        if optimizations.is_empty() {
            return false;
        }

        let unique_space_id = infer_unique_space_id(func);
        let mut next_temp_offset = initial_next_temp_offset(func, unique_space_id);

        // Apply narrowed operations from back to front to avoid shifting earlier op_idx values.
        for (block_idx, op_idx, target_size) in optimizations.into_iter().rev() {
            let block = &mut func.blocks[block_idx];
            let op = &block.ops[op_idx];

            let out_vn = op.output.as_ref().unwrap().clone();

            // Allocate fresh unique temps so narrowed intermediates never alias physical
            // registers or collide when multiple inputs share storage.
            let narrowed_out = alloc_temp(&mut next_temp_offset, unique_space_id, target_size);

            let mut new_inputs = Vec::new();
            let mut subpiece_ops = Vec::new();

            for input in &op.inputs {
                if input.is_constant {
                    let masked =
                        truncate_to_varnode_width(input.constant_val as u64, target_size) as i64;
                    new_inputs.push(Varnode::constant(masked, target_size));
                } else if input.size == target_size {
                    new_inputs.push(input.clone());
                } else if input.size < target_size {
                    let zext_out = alloc_temp(&mut next_temp_offset, unique_space_id, target_size);

                    let zext_op = PcodeOp {
                        seq_num: op.seq_num,
                        opcode: PcodeOpcode::IntZExt,
                        address: op.address,
                        output: Some(zext_out.clone()),
                        inputs: vec![input.clone()],
                        asm_mnemonic: None,
                    };
                    subpiece_ops.push(zext_op);
                    new_inputs.push(zext_out);
                } else {
                    // input.size > target_size
                    let subpiece_out =
                        alloc_temp(&mut next_temp_offset, unique_space_id, target_size);

                    // SubPiece takes 2 inputs: original value, and CONST offset (in bytes).
                    let offset_const = Varnode::constant(0, 4);

                    let subpiece_op = PcodeOp {
                        seq_num: op.seq_num,
                        opcode: PcodeOpcode::SubPiece,
                        address: op.address,
                        output: Some(subpiece_out.clone()),
                        inputs: vec![input.clone(), offset_const],
                        asm_mnemonic: None,
                    };
                    subpiece_ops.push(subpiece_op);
                    new_inputs.push(subpiece_out);
                }
            }

            // Create narrowed operation
            let mut narrowed_op = block.ops[op_idx].clone();
            narrowed_op.inputs = new_inputs;
            narrowed_op.output = Some(narrowed_out.clone());

            // Create the ZExt bridge back to the original size
            let bridge_op = PcodeOp {
                seq_num: op.seq_num,
                opcode: PcodeOpcode::IntZExt,
                address: op.address,
                output: Some(out_vn),
                inputs: vec![narrowed_out],
                asm_mnemonic: None,
            };

            // Replace the original op with:
            // 1. subpiece_ops (if any)
            // 2. narrowed_op
            // 3. bridge_op
            let mut total_insertion = subpiece_ops;
            total_insertion.push(narrowed_op);
            total_insertion.push(bridge_op);

            block.ops.splice(op_idx..=op_idx, total_insertion);
            modified = true;
        }

        modified
    }

    fn can_narrow_opcode(opcode: PcodeOpcode) -> bool {
        // Expanding a bare Copy into SubPiece + Copy + IntZExt increases op count
        // without simplifying arithmetic.
        matches!(
            opcode,
            PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
        )
    }
}
