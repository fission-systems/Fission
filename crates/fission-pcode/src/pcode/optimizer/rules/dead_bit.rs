use crate::pcode::optimizer::def_use::DefUseTracker;
use crate::pcode::{PcodeOp, PcodeOpcode};

/// Rule: Dead Bit Elimination
///
/// Removes bitwise operations that do not affect the consumed bits of the output.
/// Returns Some(new_op) if optimization is applied.
pub fn try_dead_bit_elimination(op: &PcodeOp, tracker: &DefUseTracker) -> Option<PcodeOp> {
    let out = op.output.as_ref()?;
    let consume_mask = tracker.get_consume_mask(out);

    // If output is completely unused, let DCE handle it.
    if consume_mask == 0 {
        return None;
    }

    match op.opcode {
        PcodeOpcode::IntAnd => {
            // Check for A = B & Constant
            if op.inputs.len() == 2 && op.inputs[1].is_constant {
                let mask = op.inputs[1].constant_val as u64;
                let input_nz = tracker.get_nz_mask(&op.inputs[0]);

                // Check if the mask is redundant.
                // Condition: (~Mask) & Input_NZ & Consume == 0

                let cleared_bits = !mask;
                let problematic_bits = cleared_bits & input_nz & consume_mask;

                if problematic_bits == 0 {
                    // The AND operation does not change any consumed bits that might be non-zero.
                    // Replace with COPY.
                    let mut new_op = op.clone();
                    new_op.opcode = PcodeOpcode::Copy;
                    new_op.inputs.pop(); // Remove constant
                    return Some(new_op);
                }
            }
        }

        PcodeOpcode::IntOr | PcodeOpcode::IntXor => {
            // Check for A = B | Constant or A = B ^ Constant
            if op.inputs.len() == 2 && op.inputs[1].is_constant {
                let mask = op.inputs[1].constant_val as u64;

                // Redundant if the constant mask only affects bits that are NOT consumed.
                // Condition: (Mask & Consume) == 0

                if (mask & consume_mask) == 0 {
                    // The operation only affects unconsumed bits.
                    // Replace with COPY.
                    let mut new_op = op.clone();
                    new_op.opcode = PcodeOpcode::Copy;
                    new_op.inputs.pop(); // Remove constant
                    return Some(new_op);
                }
            }
        }

        _ => {}
    }

    None
}
