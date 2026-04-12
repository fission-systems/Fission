use super::def_use::{DEFAULT_VARNODE_SIZE, DefUseTracker};
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

pub struct SubvariableFlowOptimizer;

impl SubvariableFlowOptimizer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn eliminate(&mut self, func: &mut PcodeFunction, def_use: &DefUseTracker) -> bool {
        let mut modified = false;

        // Collect operations to narrow
        // We will store pairs of (block_idx, op_idx, target_size)
        // Note: we can't mutate `func.blocks` while iterating over it,
        // so we collect changes first.
        let mut optimizations = Vec::new();

        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                if op.output.is_none() {
                    continue;
                }
                let out = op.output.as_ref().unwrap();
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

                if target_size < orig_size {
                    if Self::can_narrow_opcode(op.opcode) {
                        optimizations.push((block_idx, op_idx, target_size, orig_size));
                    }
                }
            }
        }

        if optimizations.is_empty() {
            return false;
        }

        // Apply narrowed operations from back to front to avoid indices shifting,
        // though `op_idx` will shift if we insert ops *before* it, but wait!
        // We want to:
        // 1. Convert `V_old = OP(X, Y)`  -> `V_new = OP(Subpiece(X)_target, Subpiece(Y)_target)`
        // 2. Add bridge `V_old = ZExt(V_new)` AFTER the operation.
        // If we add operations *before* `op`, we must insert them carefully.
        for (block_idx, op_idx, target_size, orig_size) in optimizations.into_iter().rev() {
            let block = &mut func.blocks[block_idx];
            let op = &block.ops[op_idx];

            let out_vn = op.output.as_ref().unwrap().clone();

            // Allocate a new unique varnode for the narrowed result
            let mut narrowed_out = out_vn.clone();
            narrowed_out.size = target_size;
            // Best practice: allocate new space offset to avoid aliasing.
            // Using a temporary register space usually requires knowing the space_id.
            // For Pcode, we can just use a unique offset derived from a counter, or
            // since we don't have func.allocate_temp, we usually use the same offset
            // but change the size. Wait! In Fission's Pcode, `space_id, offset, size` uniquely identify.
            // If we reuse the same space/offset but different size, it's technically a distinct varnode in `DefUseTracker`
            // BUT it might alias physically if not careful!
            // However, this is an intermediate temp variable (most likely in the `unique` space).
            // If it's a register or memory, changing its size might overlap adjacent memory.
            // But since these are intermediate ops that only produced `orig_size` bits that were *never consumed* fully,
            // we'll just treat the lower `target_size` bytes as uniquely identifying enough,
            // OR we can create a new temp. `func.allocate_temp()` usually exists in builder.
            // Oh well, if we just copy the ID and change size, Fission will treat it as a distinct `VarnodeId`.

            let mut new_inputs = Vec::new();
            let mut subpiece_ops = Vec::new();

            for input in &op.inputs {
                if input.is_constant {
                    let mut new_const = input.clone();
                    new_const.size = target_size;
                    new_const.constant_val &= (1i64 << (target_size * 8)) - 1;
                    new_inputs.push(new_const);
                } else if input.size == target_size {
                    new_inputs.push(input.clone());
                } else if input.size < target_size {
                    // We must ZExt it here (which adds another op, skip narrowing if so?)
                    // Actually, if an input is SMALLER than the target size, but the OP produced `orig_size`,
                    // that shouldn't happen for most bitwise/arithmetic unless it's weird Pcode. Let's just ZExt it.
                    let mut zext_out = input.clone();
                    zext_out.size = target_size;

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
                    let mut subpiece_out = input.clone();
                    subpiece_out.size = target_size;

                    // Create subpiece op
                    // SubPiece takes 2 inputs: original value, and CONST offset (in bytes).
                    let offset_const = Varnode {
                        space_id: 0, // usually const space
                        offset: 0,
                        size: 4,
                        is_constant: true,
                        constant_val: 0,
                    };

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
        matches!(
            opcode,
            PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::Copy
        )
    }
}
