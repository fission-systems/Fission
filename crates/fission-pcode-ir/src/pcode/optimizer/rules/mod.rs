//! Optimization rules for Pcode operations
//!
//! This module contains the core optimization logic, organized by operation type:
//! - Arithmetic operations (ADD, SUB, MULT, DIV, etc.)
//! - Bitwise operations (XOR, AND, OR, shifts)
//! - Boolean operations (AND, OR, XOR, NEGATE)
//! - Comparison operations (EQUAL, LESS, etc.)
//! - Constant folding for all operation types

use super::passes::def_use::{DEFAULT_VARNODE_SIZE, DefUseTracker, OpRef};
use crate::pcode::{
    PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode, truncate_to_varnode_width,
};

mod dead_bit;

/// Container for all optimization rules
pub struct OptimizationRules {
    // Can add statistics or configuration here later
}

fn is_intra_block_folding_boundary(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::Return
            | PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
    )
}

fn find_safe_intra_block_def<'a>(
    func: &'a PcodeFunction,
    use_ref: OpRef,
    target: &Varnode,
) -> Option<(usize, &'a PcodeOp)> {
    if target.is_constant {
        return None;
    }
    let block = func.blocks.get(use_ref.block_idx)?;
    for idx in (0..use_ref.op_idx).rev() {
        let candidate = &block.ops[idx];
        if is_intra_block_folding_boundary(candidate.opcode) {
            return None;
        }
        if let Some(out) = &candidate.output {
            if out == target {
                return Some((idx, candidate));
            }
            if out.overlaps(target) {
                return None;
            }
        }
    }
    None
}

fn inputs_unclobbered_between(
    block: &PcodeBasicBlock,
    def_idx: usize,
    use_idx: usize,
    def_op: &PcodeOp,
    operands: &[&Varnode],
) -> bool {
    if let Some(def_out) = &def_op.output
        && operands.iter().any(|operand| def_out.overlaps(operand))
    {
        return false;
    }
    for op in &block.ops[(def_idx + 1)..use_idx] {
        if is_intra_block_folding_boundary(op.opcode) {
            return false;
        }
        if let Some(out) = &op.output
            && operands.iter().any(|operand| out.overlaps(operand))
        {
            return false;
        }
    }
    true
}

/// A constant read at its own width, as an unsigned number. A narrow constant
/// is held either zero-extended or sign-extended in `constant_val`; both mean
/// the same p-code value.
fn unsigned_value(v: &Varnode) -> u64 {
    let bits = v.size.saturating_mul(8);
    match bits {
        1..=63 => (v.constant_val as u64) & ((1u64 << bits) - 1),
        _ => v.constant_val as u64,
    }
}

/// The same constant as a signed number: the top bit of its width is the sign.
fn signed_value(v: &Varnode) -> i64 {
    let bits = v.size.saturating_mul(8);
    match bits {
        1..=63 => {
            let shift = 64 - bits;
            ((v.constant_val as u64) << shift) as i64 >> shift
        }
        _ => v.constant_val,
    }
}

fn safe_shl_u64(value: u64, shift: u32) -> u64 {
    value.checked_shl(shift).unwrap_or(0)
}

fn safe_shr_u64(value: u64, shift: u32) -> u64 {
    value.checked_shr(shift).unwrap_or(0)
}

impl OptimizationRules {
    pub fn new() -> Self {
        Self {}
    }

    /// Try to optimize with def-use tracking at a specific operation location (Phase 2 rules)
    pub fn try_optimize_with_tracker_at(
        &self,
        op: &PcodeOp,
        use_ref: OpRef,
        tracker: &DefUseTracker,
        func: &PcodeFunction,
    ) -> Option<PcodeOp> {
        self.try_optimize_with_tracker_inner(op, Some(use_ref), tracker, func)
    }

    /// Try to optimize with def-use tracking (Phase 2 rules)
    pub fn try_optimize_with_tracker(
        &self,
        op: &PcodeOp,
        tracker: &DefUseTracker,
        func: &PcodeFunction,
    ) -> Option<PcodeOp> {
        let mut matched: Option<OpRef> = None;
        let mut ambiguous = false;
        'outer: for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, candidate) in block.ops.iter().enumerate() {
                if candidate == op {
                    if matched.is_some() {
                        ambiguous = true;
                        break 'outer;
                    }
                    matched = Some(OpRef { block_idx, op_idx });
                }
            }
        }
        let use_ref = if ambiguous { None } else { matched };
        self.try_optimize_with_tracker_inner(op, use_ref, tracker, func)
    }

    fn try_optimize_with_tracker_inner(
        &self,
        op: &PcodeOp,
        use_ref: Option<OpRef>,
        tracker: &DefUseTracker,
        func: &PcodeFunction,
    ) -> Option<PcodeOp> {
        // Try advanced rules first (they're more specific)
        if let Some(result) = dead_bit::try_dead_bit_elimination(op, tracker) {
            return Some(result);
        }
        if let Some(result) = self.try_shift_bitops(op, tracker) {
            return Some(result);
        }
        if let Some(result) = self.try_and_mask(op, tracker) {
            return Some(result);
        }
        if let Some(use_ref) = use_ref {
            if let Some(result) = self.try_ptr_arith(op, use_ref, func) {
                return Some(result);
            }
            if let Some(result) = self.try_pull_sub_indirect(op, use_ref, func) {
                return Some(result);
            }
            if let Some(result) = self.try_indirect_collapse(op, use_ref, func) {
                return Some(result);
            }
        }

        // Fall back to basic rules
        self.try_optimize(op)
    }

    /// Try to optimize a single Pcode operation (Phase 1 rules)
    pub fn try_optimize(&self, op: &PcodeOp) -> Option<PcodeOp> {
        match op.opcode {
            // Bitwise operations
            PcodeOpcode::IntXor => self.optimize_xor(op),
            PcodeOpcode::IntAnd => self.optimize_and(op),
            PcodeOpcode::IntOr => self.optimize_or(op),

            // Arithmetic operations
            PcodeOpcode::IntAdd => self.optimize_add(op),
            PcodeOpcode::IntSub => self.optimize_sub(op),
            PcodeOpcode::IntMult => self.optimize_mult(op),
            PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => self.optimize_div(op),
            PcodeOpcode::IntRem | PcodeOpcode::IntSRem => self.optimize_rem(op),

            // Shift operations
            PcodeOpcode::IntLeft => self.optimize_left_shift(op),
            PcodeOpcode::IntRight => self.optimize_right_shift(op),
            PcodeOpcode::IntSRight => self.optimize_sright_shift(op),

            // Comparison operations
            PcodeOpcode::IntEqual => self.optimize_equal(op),
            PcodeOpcode::IntNotEqual => self.optimize_not_equal(op),
            PcodeOpcode::IntLess => self.optimize_less(op),
            PcodeOpcode::IntSLess => self.optimize_sless(op),
            PcodeOpcode::IntLessEqual => self.optimize_less_equal(op),
            PcodeOpcode::IntSLessEqual => self.optimize_sless_equal(op),

            // Boolean operations
            PcodeOpcode::BoolAnd | PcodeOpcode::BoolOr => self.optimize_bool_and_or(op),
            PcodeOpcode::BoolXor => self.optimize_bool_xor(op),

            // Float comparisons
            PcodeOpcode::FloatEqual => self.optimize_float_equal(op),
            PcodeOpcode::FloatNotEqual | PcodeOpcode::FloatLess => {
                self.optimize_float_not_equal_less(op)
            }
            PcodeOpcode::FloatLessEqual => self.optimize_float_less_equal(op),

            _ => None,
        }
    }

    // ===== Bitwise Operations =====

    fn optimize_xor(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x ^ 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_zero() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // x ^ x => 0
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_constant(op, 0));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = (unsigned_value(&op.inputs[0]) ^ unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_and(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x & 0 => 0
        if op.inputs[1].is_zero() || op.inputs[0].is_zero() {
            return Some(self.make_constant(op, 0));
        }

        // x & -1 => x
        if op.inputs[1].is_all_ones() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_all_ones() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // x & x => x
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = (unsigned_value(&op.inputs[0]) & unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_or(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x | 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_zero() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // x | -1 => -1
        if op.inputs[1].is_all_ones() || op.inputs[0].is_all_ones() {
            let all_ones = if let Some(out) = &op.output {
                out.size
            } else {
                4
            };
            // A 64-bit (or wider) result is all-ones in the whole i64; shifting by
            // 64 would overflow (panic with overflow checks, wrap to 0 without).
            let all_ones_val = match all_ones.checked_mul(8) {
                Some(bits) if bits < 64 => (1i64 << bits) - 1,
                _ => -1,
            };
            return Some(self.make_constant(op, all_ones_val));
        }

        // x | x => x
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = (unsigned_value(&op.inputs[0]) | unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    // ===== Arithmetic Operations =====

    fn optimize_add(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x + 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_zero() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // Constant folding: const + const
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result =
                unsigned_value(&op.inputs[0]).wrapping_add(unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_sub(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x - 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // x - x => 0
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_constant(op, 0));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result =
                unsigned_value(&op.inputs[0]).wrapping_sub(unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_mult(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x * 0 => 0
        if op.inputs[1].is_zero() || op.inputs[0].is_zero() {
            return Some(self.make_constant(op, 0));
        }

        // x * 1 => x
        if op.inputs[1].is_one() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_one() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result =
                unsigned_value(&op.inputs[0]).wrapping_mul(unsigned_value(&op.inputs[1])) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_div(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x / 1 => x
        if op.inputs[1].is_one() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding (guard zero divisor after masking to varnode width)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = if op.opcode == PcodeOpcode::IntDiv {
                let rhs = unsigned_value(&op.inputs[1]);
                if rhs == 0 {
                    return None;
                }
                unsigned_value(&op.inputs[0]).wrapping_div(rhs) as i64
            } else {
                let rhs = signed_value(&op.inputs[1]);
                if rhs == 0 {
                    return None;
                }
                signed_value(&op.inputs[0]).wrapping_div(rhs)
            };
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_rem(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x % 1 => 0
        if op.inputs[1].is_one() {
            return Some(self.make_constant(op, 0));
        }

        // Constant folding (guard zero divisor after masking to varnode width)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = if op.opcode == PcodeOpcode::IntRem {
                let rhs = unsigned_value(&op.inputs[1]);
                if rhs == 0 {
                    return None;
                }
                unsigned_value(&op.inputs[0]).wrapping_rem(rhs) as i64
            } else {
                let rhs = signed_value(&op.inputs[1]);
                if rhs == 0 {
                    return None;
                }
                signed_value(&op.inputs[0]).wrapping_rem(rhs)
            };
            return Some(self.make_constant(op, result));
        }

        None
    }

    // ===== Shift Operations =====

    fn optimize_left_shift(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x << 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let shift = unsigned_value(&op.inputs[1]);
            let result = u32::try_from(shift)
                .ok()
                .map_or(0, |s| safe_shl_u64(unsigned_value(&op.inputs[0]), s))
                as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_right_shift(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x >> 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding (unsigned shift)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = unsigned_value(&op.inputs[0])
                .wrapping_shr(unsigned_value(&op.inputs[1]) as u32) as i64;
            return Some(self.make_constant(op, result));
        }

        None
    }

    fn optimize_sright_shift(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x >> 0 => x
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        // Constant folding (signed shift)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result =
                signed_value(&op.inputs[0]).wrapping_shr(unsigned_value(&op.inputs[1]) as u32);
            return Some(self.make_constant(op, result));
        }

        None
    }

    // ===== Comparison Operations =====

    fn optimize_equal(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x == x => true
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, true));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = unsigned_value(&op.inputs[0]) == unsigned_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    fn optimize_not_equal(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x != x => false
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, false));
        }

        // Constant folding
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = unsigned_value(&op.inputs[0]) != unsigned_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    fn optimize_less(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x < x => false
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, false));
        }

        // Constant folding (unsigned)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = unsigned_value(&op.inputs[0]) < unsigned_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    fn optimize_sless(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x < x => false
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, false));
        }

        // Constant folding (signed)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = signed_value(&op.inputs[0]) < signed_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    fn optimize_less_equal(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x <= x => true
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, true));
        }

        // Constant folding (unsigned)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = unsigned_value(&op.inputs[0]) <= unsigned_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    fn optimize_sless_equal(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // x <= x => true
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, true));
        }

        // Constant folding (signed)
        if op.inputs[0].is_constant && op.inputs[1].is_constant {
            let result = signed_value(&op.inputs[0]) <= signed_value(&op.inputs[1]);
            return Some(self.make_boolean(op, result));
        }

        None
    }

    // ===== Boolean Operations =====

    fn optimize_bool_and_or(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // V && V => V, V || V => V
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_copy(op, &op.inputs[0]));
        }

        if op.opcode == PcodeOpcode::BoolAnd {
            // V && true => V
            if op.inputs[1].is_one() {
                return Some(self.make_copy(op, &op.inputs[0]));
            }
            if op.inputs[0].is_one() {
                return Some(self.make_copy(op, &op.inputs[1]));
            }

            // V && false => false
            if op.inputs[1].is_zero() || op.inputs[0].is_zero() {
                return Some(self.make_boolean(op, false));
            }
        }

        if op.opcode == PcodeOpcode::BoolOr {
            // V || false => V
            if op.inputs[1].is_zero() {
                return Some(self.make_copy(op, &op.inputs[0]));
            }
            if op.inputs[0].is_zero() {
                return Some(self.make_copy(op, &op.inputs[1]));
            }

            // V || true => true
            if op.inputs[1].is_one() || op.inputs[0].is_one() {
                return Some(self.make_boolean(op, true));
            }
        }

        None
    }

    fn optimize_bool_xor(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.inputs.len() != 2 {
            return None;
        }

        // V ^^ V => false
        if op.inputs[0] == op.inputs[1] {
            return Some(self.make_boolean(op, false));
        }

        // V ^^ false => V
        if op.inputs[1].is_zero() {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if op.inputs[0].is_zero() {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        // V ^^ true => !V
        if op.inputs[1].is_one() {
            return Some(PcodeOp {
                seq_num: op.seq_num,
                opcode: PcodeOpcode::BoolNegate,
                address: op.address,
                output: op.output.clone(),
                inputs: vec![op.inputs[0].clone()],
                asm_mnemonic: op.asm_mnemonic.clone(),
            });
        }
        if op.inputs[0].is_one() {
            return Some(PcodeOp {
                seq_num: op.seq_num,
                opcode: PcodeOpcode::BoolNegate,
                address: op.address,
                output: op.output.clone(),
                inputs: vec![op.inputs[1].clone()],
                asm_mnemonic: op.asm_mnemonic.clone(),
            });
        }

        None
    }

    // ===== Float Comparisons =====

    // `x == x`, `x <= x` and `x != x` are not constants for floats: a NaN is
    // unordered, so `NaN == NaN` and `NaN <= NaN` are false and `NaN != NaN` is
    // true. Folding them to the integer answers erases the `x != x` NaN test.
    fn optimize_float_equal(&self, _op: &PcodeOp) -> Option<PcodeOp> {
        None
    }

    /// `x < x` is false for every `x`, NaN included; `x != x` is not constant.
    fn optimize_float_not_equal_less(&self, op: &PcodeOp) -> Option<PcodeOp> {
        if op.opcode == PcodeOpcode::FloatLess
            && op.inputs.len() == 2
            && op.inputs[0] == op.inputs[1]
        {
            return Some(self.make_boolean(op, false));
        }
        None
    }

    fn optimize_float_less_equal(&self, _op: &PcodeOp) -> Option<PcodeOp> {
        None
    }

    // ===== Helper Methods =====

    /// Create a COPY operation
    fn make_copy(&self, original: &PcodeOp, source: &Varnode) -> PcodeOp {
        PcodeOp {
            seq_num: original.seq_num,
            opcode: PcodeOpcode::Copy,
            address: original.address,
            output: original.output.clone(),
            inputs: vec![source.clone()],
            asm_mnemonic: original.asm_mnemonic.clone(),
        }
    }

    /// Create a constant load operation, masking the constant value to the target width.
    fn make_constant(&self, original: &PcodeOp, value: i64) -> PcodeOp {
        let size = if let Some(out) = &original.output {
            out.size
        } else {
            4
        };
        let truncated = truncate_to_varnode_width(value as u64, size) as i64;

        PcodeOp {
            seq_num: original.seq_num,
            opcode: PcodeOpcode::Copy,
            address: original.address,
            output: original.output.clone(),
            inputs: vec![Varnode::constant(truncated, size)],
            asm_mnemonic: original.asm_mnemonic.clone(),
        }
    }

    /// Create a boolean constant (1 byte size, 0 or 1)
    fn make_boolean(&self, original: &PcodeOp, value: bool) -> PcodeOp {
        PcodeOp {
            seq_num: original.seq_num,
            opcode: PcodeOpcode::Copy,
            address: original.address,
            output: original.output.clone(),
            inputs: vec![Varnode::constant(if value { 1 } else { 0 }, 1)],
            asm_mnemonic: original.asm_mnemonic.clone(),
        }
    }

    // ===== Phase 2: Advanced Rules with Def-Use Tracking =====

    /// RuleShiftBitops: Optimize shifts where all non-zero bits are shifted out
    /// Example: (V & 0xf000) << 4 => #0 (all bits shifted out of range)
    fn try_shift_bitops(&self, op: &PcodeOp, tracker: &DefUseTracker) -> Option<PcodeOp> {
        // This rule applies to shift operations
        let (is_left, shift_amt) = match op.opcode {
            PcodeOpcode::IntLeft => {
                if op.inputs.len() < 2 || !op.inputs[1].is_constant {
                    return None;
                }
                (true, op.inputs[1].constant_val as u32)
            }
            PcodeOpcode::IntRight | PcodeOpcode::IntSRight => {
                if op.inputs.len() < 2 || !op.inputs[1].is_constant {
                    return None;
                }
                (false, op.inputs[1].constant_val as u32)
            }
            _ => return None,
        };

        // Check if all non-zero bits would be shifted out
        let input_nz = tracker.get_nz_mask(&op.inputs[0]);
        let out_size = op
            .output
            .as_ref()
            .map(|v| v.size)
            .unwrap_or(DEFAULT_VARNODE_SIZE);
        let out_mask = match out_size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            8 => u64::MAX,
            _ => u64::MAX,
        };

        // Simulate the shift
        let shifted_nz = if is_left {
            safe_shl_u64(input_nz, shift_amt) & out_mask
        } else {
            safe_shr_u64(input_nz, shift_amt)
        };

        // If all non-zero bits are gone, result is 0
        if shifted_nz == 0 {
            return Some(self.make_constant(op, 0));
        }

        None
    }

    /// RuleAndMask: Optimize AND operations using NZ masks
    /// Examples:
    /// - V & 0xff => V when V's NZMask is 0x0f (AND has no effect)
    /// - V & mask => #0 when (V's NZMask & mask) == 0
    fn try_and_mask(&self, op: &PcodeOp, tracker: &DefUseTracker) -> Option<PcodeOp> {
        if op.opcode != PcodeOpcode::IntAnd || op.inputs.len() < 2 {
            return None;
        }

        let mask1 = tracker.get_nz_mask(&op.inputs[0]);
        let mask2 = tracker.get_nz_mask(&op.inputs[1]);
        let and_mask = mask1 & mask2;

        // If result is always 0
        if and_mask == 0 {
            return Some(self.make_constant(op, 0));
        }

        // Check consume mask
        if let Some(out) = &op.output {
            let consume = tracker.get_consume_mask(out);
            // If `out` is live/consumed, fold when no consumed bits can be nonzero.
            if consume != 0 && (and_mask & consume) == 0 {
                return Some(self.make_constant(op, 0));
            }
        }

        // If AND doesn't clear any bits (no-op)
        if and_mask == mask1 {
            return Some(self.make_copy(op, &op.inputs[0]));
        }
        if and_mask == mask2 && !op.inputs[1].is_constant {
            return Some(self.make_copy(op, &op.inputs[1]));
        }

        None
    }

    /// RulePtrArith: Optimize pointer arithmetic
    /// (A + c1) + c2 => A + (c1 + c2)
    fn try_ptr_arith(&self, op: &PcodeOp, use_ref: OpRef, func: &PcodeFunction) -> Option<PcodeOp> {
        if op.opcode != PcodeOpcode::IntAdd || op.inputs.len() != 2 {
            return None;
        }

        // Check if one input is constant
        let (const_idx, var_idx) = if op.inputs[1].is_constant {
            (1, 0)
        } else if op.inputs[0].is_constant {
            (0, 1)
        } else {
            return None;
        };

        let const_size = op.inputs[const_idx].size;
        let c2 = unsigned_value(&op.inputs[const_idx]);
        let var_input = &op.inputs[var_idx];

        let (def_idx, def_op) = find_safe_intra_block_def(func, use_ref, var_input)?;
        if def_op.opcode != PcodeOpcode::IntAdd || def_op.inputs.len() != 2 {
            return None;
        }
        let c1_idx = def_op.inputs.iter().position(|v| v.is_constant)?;
        if def_op.inputs[c1_idx].size != const_size {
            return None;
        }
        let c1 = unsigned_value(&def_op.inputs[c1_idx]);
        let base_idx = 1 - c1_idx;
        let base = &def_op.inputs[base_idx];

        let block = func.blocks.get(use_ref.block_idx)?;
        if !inputs_unclobbered_between(block, def_idx, use_ref.op_idx, def_op, &[base]) {
            return None;
        }

        let new_c = truncate_to_varnode_width(c1.wrapping_add(c2), const_size) as i64;
        let new_inputs = vec![base.clone(), Varnode::constant(new_c, const_size)];

        Some(PcodeOp {
            seq_num: op.seq_num,
            opcode: PcodeOpcode::IntAdd,
            address: op.address,
            output: op.output.clone(),
            inputs: new_inputs,
            asm_mnemonic: op.asm_mnemonic.clone(),
        })
    }

    /// RulePullSubIndirect: Optimize (ptr + off) - ptr => off
    fn try_pull_sub_indirect(
        &self,
        op: &PcodeOp,
        use_ref: OpRef,
        func: &PcodeFunction,
    ) -> Option<PcodeOp> {
        if op.opcode != PcodeOpcode::IntSub || op.inputs.len() != 2 {
            return None;
        }

        let ptr_plus_off = &op.inputs[0];
        let ptr = &op.inputs[1];

        let (def_idx, def_op) = find_safe_intra_block_def(func, use_ref, ptr_plus_off)?;
        if def_op.opcode != PcodeOpcode::IntAdd || def_op.inputs.len() != 2 {
            return None;
        }

        let off = if def_op.inputs[0] == *ptr {
            &def_op.inputs[1]
        } else if def_op.inputs[1] == *ptr {
            &def_op.inputs[0]
        } else {
            return None;
        };

        let block = func.blocks.get(use_ref.block_idx)?;
        if !inputs_unclobbered_between(block, def_idx, use_ref.op_idx, def_op, &[ptr, off]) {
            return None;
        }

        Some(self.make_copy(op, off))
    }

    /// RuleIndirectCollapse: Simplify indirect calculations
    /// PTRSUB(PTRSUB(base, c1), c2) => PTRSUB(base, c1+c2)
    fn try_indirect_collapse(
        &self,
        op: &PcodeOp,
        use_ref: OpRef,
        func: &PcodeFunction,
    ) -> Option<PcodeOp> {
        if op.opcode != PcodeOpcode::PtrSub || op.inputs.len() != 2 {
            return None;
        }

        // PTRSUB(base, offset)
        let base = &op.inputs[0];
        let offset = &op.inputs[1];

        if !offset.is_constant {
            return None;
        }
        let c2 = unsigned_value(offset);

        let (def_idx, def_op) = find_safe_intra_block_def(func, use_ref, base)?;
        if def_op.opcode != PcodeOpcode::PtrSub || def_op.inputs.len() != 2 {
            return None;
        }
        let inner_base = &def_op.inputs[0];
        let inner_offset = &def_op.inputs[1];
        if !inner_offset.is_constant || inner_offset.size != offset.size {
            return None;
        }
        let block = func.blocks.get(use_ref.block_idx)?;
        if !inputs_unclobbered_between(block, def_idx, use_ref.op_idx, def_op, &[inner_base]) {
            return None;
        }

        let c1 = unsigned_value(inner_offset);
        let new_c = truncate_to_varnode_width(c1.wrapping_add(c2), offset.size) as i64;
        let new_inputs = vec![inner_base.clone(), Varnode::constant(new_c, offset.size)];

        Some(PcodeOp {
            seq_num: op.seq_num,
            opcode: PcodeOpcode::PtrSub,
            address: op.address,
            output: op.output.clone(),
            inputs: new_inputs,
            asm_mnemonic: op.asm_mnemonic.clone(),
        })
    }
}
