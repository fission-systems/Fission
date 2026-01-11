use crate::pcode::{PcodeFunction, PcodeOpcode, Varnode};
use std::collections::HashMap;

/// Common Subexpression Elimination (CSE)
///
/// Identifies and removes redundant computations by hashing operations.
/// If an operation computes the same value as a previous operation,
/// it is replaced with a COPY from the previous result.
pub struct CommonSubexpressionEliminator {
    // Map from (opcode, inputs) to output varnode
    available_exprs: HashMap<(PcodeOpcode, Vec<Varnode>), Varnode>,
}

impl CommonSubexpressionEliminator {
    pub fn new() -> Self {
        Self {
            available_exprs: HashMap::new(),
        }
    }

    /// Apply Local CSE to the function (per basic block)
    /// Returns true if any changes were made
    pub fn eliminate(&mut self, func: &mut PcodeFunction) -> bool {
        let mut modified = false;

        for block in &mut func.blocks {
            self.available_exprs.clear();

            for op in &mut block.ops {
                // Skip operations with side effects or no output
                if op.output.is_none() {
                    continue;
                }

                if !self.is_pure_arithmetic(op.opcode) {
                    continue;
                }

                let key = (op.opcode, op.inputs.clone());

                // Check if we've seen this expression before
                let mut found = false;

                // 1. Check exact match
                if let Some(existing_output) = self.available_exprs.get(&key) {
                    self.apply_cse(op, existing_output);
                    modified = true;
                    found = true;
                }
                // 2. Check commutative match (a+b vs b+a)
                else if op.opcode.is_commutative() && op.inputs.len() == 2 {
                    let swapped_key = (op.opcode, vec![op.inputs[1].clone(), op.inputs[0].clone()]);
                    if let Some(existing_output) = self.available_exprs.get(&swapped_key) {
                        self.apply_cse(op, existing_output);
                        modified = true;
                        found = true;
                    }
                }

                // If not found, register this expression
                if !found {
                    if let Some(out) = &op.output {
                        self.available_exprs.insert(key, out.clone());
                    }
                }
            }
        }

        modified
    }

    fn apply_cse(&self, op: &mut crate::pcode::PcodeOp, existing_output: &Varnode) {
        // Replace with COPY
        op.opcode = PcodeOpcode::Copy;
        op.inputs = vec![existing_output.clone()];
        // Output remains the same
    }

    fn is_pure_arithmetic(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntDiv
                | PcodeOpcode::IntSDiv
                | PcodeOpcode::IntRem
                | PcodeOpcode::IntSRem
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::BoolXor
                | PcodeOpcode::FloatAdd
                | PcodeOpcode::FloatSub
                | PcodeOpcode::FloatMult
                | PcodeOpcode::FloatDiv
                | PcodeOpcode::IntNegate
                | PcodeOpcode::Int2Comp
                | PcodeOpcode::BoolNegate
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
        )
    }
}
