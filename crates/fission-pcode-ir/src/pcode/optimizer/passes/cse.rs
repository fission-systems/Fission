use crate::pcode::{PcodeFunction, PcodeOpcode, Varnode};
use std::collections::HashMap;

/// Common Subexpression Elimination (CSE)
///
/// Identifies and removes redundant computations by hashing operations.
/// If an operation computes the same value as a previous operation,
/// it is replaced with a COPY from the previous result.
pub struct CommonSubexpressionEliminator {
    // Map from (opcode, inputs, output size) to the varnode that holds the value.
    // The output size is part of the key: `IntZExt(x:1)` into 4 bytes and into 8
    // bytes are different values, and rewriting one into a COPY of the other
    // hands back a value of the wrong width.
    available_exprs: HashMap<(PcodeOpcode, Vec<Varnode>, Option<u32>), Varnode>,
}

/// Whether two varnodes name overlapping storage. Constants name none.
fn varnodes_overlap(a: &Varnode, b: &Varnode) -> bool {
    if a.is_constant || b.is_constant || a.space_id != b.space_id {
        return false;
    }
    let (a_end, b_end) = (
        a.offset.saturating_add(u64::from(a.size.max(1))),
        b.offset.saturating_add(u64::from(b.size.max(1))),
    );
    a.offset < b_end && b.offset < a_end
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
                // This is not SSA: registers and reused `unique` varnodes are
                // written again inside a block. An expression stays available
                // only while neither its inputs nor the varnode holding its
                // value have been overwritten, so every write kills what it
                // overlaps -- including the writes of ops that are not CSE
                // candidates themselves.
                if matches!(
                    op.opcode,
                    PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::CallOther
                        | PcodeOpcode::Store
                ) {
                    // A call or a store can change anything an expression reads.
                    self.available_exprs.clear();
                }

                // Skip operations with side effects or no output
                let Some(written) = op.output.clone() else {
                    continue;
                };

                if !self.is_pure_arithmetic(op.opcode) {
                    self.kill_overlapping(&written);
                    continue;
                }

                let key = (op.opcode, op.inputs.clone(), Some(written.size));

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
                    let swapped_key = (
                        op.opcode,
                        vec![op.inputs[1].clone(), op.inputs[0].clone()],
                        Some(written.size),
                    );
                    if let Some(existing_output) = self.available_exprs.get(&swapped_key) {
                        self.apply_cse(op, existing_output);
                        modified = true;
                        found = true;
                    }
                }

                // This op overwrites its output, whatever it computed.
                self.kill_overlapping(&written);

                // If not found, register this expression -- unless its own result
                // overwrote one of its inputs, in which case `key` describes the
                // old input value and no longer matches what is stored.
                if !found
                    && !op
                        .inputs
                        .iter()
                        .any(|input| varnodes_overlap(input, &written))
                {
                    self.available_exprs.insert(key, written);
                }
            }
        }

        modified
    }

    /// Forget every expression that reads, or is held in, storage `written` overlaps.
    fn kill_overlapping(&mut self, written: &Varnode) {
        self.available_exprs.retain(|(_, inputs, _), holder| {
            !varnodes_overlap(holder, written)
                && !inputs.iter().any(|input| varnodes_overlap(input, written))
        });
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
