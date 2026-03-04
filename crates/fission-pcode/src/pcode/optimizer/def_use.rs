//! Def-use chain tracking and analysis for Pcode optimization
//!
//! This module provides infrastructure for:
//! - Tracking which operations define and use each varnode
//! - Computing non-zero masks (NZMask) for values
//! - Enabling advanced optimizations like CSE

use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::collections::HashMap;

/// Default varnode size (4 bytes = 32-bit) used when size cannot be determined
pub(super) const DEFAULT_VARNODE_SIZE: u32 = 4;

/// Unique identifier for a varnode across all blocks
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct VarnodeId {
    pub space_id: u64,
    pub offset: u64,
    pub size: u32,
}

impl From<&Varnode> for VarnodeId {
    fn from(vn: &Varnode) -> Self {
        Self {
            space_id: vn.space_id,
            offset: vn.offset,
            size: vn.size,
        }
    }
}

/// Reference to a specific operation in a block
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct OpRef {
    pub block_idx: usize,
    pub op_idx: usize,
}

/// Def-use information for a varnode
#[derive(Debug, Clone)]
pub struct DefUseInfo {
    /// Operation that defines this varnode (writer)
    pub def: Option<OpRef>,
    /// Operations that use this varnode (readers)
    pub uses: Vec<OpRef>,
    /// Non-zero mask: bits that can be non-zero
    pub nz_mask: u64,
    /// Consume mask: bits that are actually used by consumers
    pub consume_mask: u64,
}

/// Def-use chain tracker
pub struct DefUseTracker {
    /// Map from varnode ID to its def-use info
    def_use: HashMap<VarnodeId, DefUseInfo>,
}

impl DefUseTracker {
    pub fn new() -> Self {
        Self {
            def_use: HashMap::new(),
        }
    }

    /// Build def-use chains for a function
    pub fn build(&mut self, func: &PcodeFunction) {
        self.def_use.clear();

        // Pass 1: Find all definitions
        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                if let Some(out) = &op.output {
                    let vn_id = VarnodeId::from(out);
                    let op_ref = OpRef { block_idx, op_idx };

                    self.def_use
                        .entry(vn_id)
                        .or_insert_with(|| DefUseInfo {
                            def: Some(op_ref),
                            uses: Vec::new(),
                            nz_mask: 0,
                            consume_mask: 0,
                        })
                        .def = Some(op_ref);
                }
            }
        }

        // Pass 2: Find all uses
        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let op_ref = OpRef { block_idx, op_idx };

                for input in &op.inputs {
                    if !input.is_constant {
                        let vn_id = VarnodeId::from(input);
                        self.def_use
                            .entry(vn_id)
                            .or_insert_with(|| DefUseInfo {
                                def: None,
                                uses: Vec::new(),
                                nz_mask: 0,
                                consume_mask: 0,
                            })
                            .uses
                            .push(op_ref);
                    }
                }
            }
        }

        // Pass 3: Compute non-zero masks
        self.compute_nz_masks(func);

        // Pass 4: Compute consume masks
        self.compute_consume_masks(func);
    }

    /// Get def-use info for a varnode
    pub fn get_info(&self, vn: &Varnode) -> Option<&DefUseInfo> {
        if vn.is_constant {
            return None;
        }
        let vn_id = VarnodeId::from(vn);
        self.def_use.get(&vn_id)
    }

    /// Check if a varnode is defined (written)
    pub fn is_written(&self, vn: &Varnode) -> bool {
        self.get_info(vn).and_then(|info| info.def).is_some()
    }

    /// Get the operation that defines a varnode
    pub fn get_def(&self, vn: &Varnode) -> Option<OpRef> {
        self.get_info(vn).and_then(|info| info.def)
    }

    /// Get the uses of a varnode
    pub fn get_uses(&self, vn: &Varnode) -> Vec<OpRef> {
        self.get_info(vn)
            .map(|info| info.uses.clone())
            .unwrap_or_default()
    }

    /// Get non-zero mask for a varnode
    pub fn get_nz_mask(&self, vn: &Varnode) -> u64 {
        if vn.is_constant {
            return self.constant_nz_mask(vn);
        }
        self.get_info(vn)
            .map(|info| info.nz_mask)
            .unwrap_or(u64::MAX)
    }

    /// Get consume mask for a varnode
    pub fn get_consume_mask(&self, vn: &Varnode) -> u64 {
        if vn.is_constant {
            return u64::MAX; // Constants are always fully consumed
        }
        self.get_info(vn)
            .map(|info| info.consume_mask)
            .unwrap_or(u64::MAX)
    }

    /// Compute non-zero mask for a constant
    fn constant_nz_mask(&self, vn: &Varnode) -> u64 {
        if !vn.is_constant {
            return u64::MAX;
        }
        let mask = self.size_mask(vn.size);
        (vn.constant_val as u64) & mask
    }

    /// Get mask for a given size
    fn size_mask(&self, size: u32) -> u64 {
        match size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            8 => u64::MAX,
            _ => u64::MAX,
        }
    }

    /// Compute non-zero masks for all varnodes
    fn compute_nz_masks(&mut self, func: &PcodeFunction) {
        // Iterate multiple times until convergence
        let max_iterations = 10;
        for _ in 0..max_iterations {
            let mut changed = false;

            for block in &func.blocks {
                for op in &block.ops {
                    if let Some(out) = &op.output {
                        let old_mask = self.get_nz_mask(out);
                        let new_mask = self.compute_op_nz_mask(op);

                        if new_mask != old_mask {
                            let vn_id = VarnodeId::from(out);
                            if let Some(info) = self.def_use.get_mut(&vn_id) {
                                info.nz_mask = new_mask;
                                changed = true;
                            }
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    /// Compute NZ mask for an operation's output
    fn compute_op_nz_mask(&self, op: &PcodeOp) -> u64 {
        let out_size = op
            .output
            .as_ref()
            .map(|v| v.size)
            .unwrap_or(DEFAULT_VARNODE_SIZE);
        let mask = self.size_mask(out_size);

        match op.opcode {
            PcodeOpcode::Copy => {
                if op.inputs.is_empty() {
                    return mask;
                }
                self.get_nz_mask(&op.inputs[0])
            }

            PcodeOpcode::IntAnd => {
                if op.inputs.len() < 2 {
                    return mask;
                }
                self.get_nz_mask(&op.inputs[0]) & self.get_nz_mask(&op.inputs[1])
            }

            PcodeOpcode::IntOr => {
                if op.inputs.len() < 2 {
                    return mask;
                }
                self.get_nz_mask(&op.inputs[0]) | self.get_nz_mask(&op.inputs[1])
            }

            PcodeOpcode::IntXor => {
                if op.inputs.len() < 2 {
                    return mask;
                }
                self.get_nz_mask(&op.inputs[0]) | self.get_nz_mask(&op.inputs[1])
            }

            PcodeOpcode::IntLeft => {
                if op.inputs.len() < 2 {
                    return mask;
                }
                let shift_amt = if op.inputs[1].is_constant {
                    op.inputs[1].constant_val as u32
                } else {
                    return mask; // Unknown shift
                };
                (self.get_nz_mask(&op.inputs[0]) << shift_amt) & mask
            }

            PcodeOpcode::IntRight | PcodeOpcode::IntSRight => {
                if op.inputs.len() < 2 {
                    return mask;
                }
                let shift_amt = if op.inputs[1].is_constant {
                    op.inputs[1].constant_val as u32
                } else {
                    return mask; // Unknown shift
                };
                self.get_nz_mask(&op.inputs[0]) >> shift_amt
            }

            PcodeOpcode::IntZExt => {
                if op.inputs.is_empty() {
                    return mask;
                }
                self.get_nz_mask(&op.inputs[0])
            }

            // For other operations, assume all bits can be set
            _ => mask,
        }
    }

    /// Compute consume masks (which bits are actually used)
    fn compute_consume_masks(&mut self, func: &PcodeFunction) {
        // Initialize all consume masks to 0 (not consumed)
        // Except for return values or side-effect operations which we assume consume everything
        // For now, we'll start with 0 and propagate from "sinks"
        // But since we don't track "sinks" explicitly (like Return, Store),
        // we might need a different approach.
        //
        // Alternative: Start with u64::MAX (pessimistic) and narrow down?
        // No, consume masks should start at 0 and grow as we find uses.
        // But "uses" are inputs to other ops.
        // If an op is a "sink" (Store, Branch, Return), its inputs are fully consumed.

        // Let's stick to the current approach: Start with 0, and mark "Root" uses as fully consumed.
        // But wait, the current code started with u64::MAX.
        // "Start with all output varnodes having full consume mask" -> This assumes everything is used unless proven otherwise.
        // This is "pessimistic" (safe).
        // If we want to find "dead bits", we want to find bits that are NOT consumed.

        // Let's refine:
        // 1. Initialize all varnodes to 0 (optimistic - nothing is used).
        // 2. Identify "Critical" operations (Store, Branch, Return, Call). Mark their inputs as fully consumed (u64::MAX).
        // 3. Propagate backwards.

        let mut changed;
        let max_iterations = 10;

        // Initialize:
        // - If a varnode is an output of an op, set consume to 0.
        // - We will accumulate consume masks from uses.
        for info in self.def_use.values_mut() {
            info.consume_mask = 0;
        }

        // Iterative propagation
        for _ in 0..max_iterations {
            changed = false;

            for block in &func.blocks {
                for op in &block.ops {
                    // Determine what this op consumes from its inputs
                    let input_consume_req = match op.opcode {
                        // Sinks / Critical Ops: Consume everything
                        PcodeOpcode::Store
                        | PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                        | PcodeOpcode::BranchInd
                        | PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::Return => u64::MAX,

                        // Propagate from output's consume mask
                        _ => {
                            if let Some(out) = &op.output {
                                self.get_consume_mask(out)
                            } else {
                                u64::MAX // No output but not a sink? Assume full consume (e.g. intrinsics)
                            }
                        }
                    };

                    // Apply to inputs based on opcode logic
                    match op.opcode {
                        PcodeOpcode::Copy | PcodeOpcode::IntOr | PcodeOpcode::IntXor => {
                            // Bits consumed in output are needed from ALL inputs
                            for input in &op.inputs {
                                if self.add_consume(input, input_consume_req) {
                                    changed = true;
                                }
                            }
                        }

                        PcodeOpcode::IntAnd => {
                            // A = B & C
                            // Bits of B consumed = (Bits of A consumed) & (NZMask of C)
                            if op.inputs.len() >= 2 {
                                let mask0 = self.get_nz_mask(&op.inputs[0]);
                                let mask1 = self.get_nz_mask(&op.inputs[1]);

                                // Input 0 consumes: OutputConsume & Input1_NZ
                                if self.add_consume(&op.inputs[0], input_consume_req & mask1) {
                                    changed = true;
                                }
                                // Input 1 consumes: OutputConsume & Input0_NZ
                                if self.add_consume(&op.inputs[1], input_consume_req & mask0) {
                                    changed = true;
                                }
                            }
                        }

                        PcodeOpcode::IntLeft => {
                            // A = B << C
                            // Bits of B consumed = (Bits of A consumed) >> C
                            // C is fully consumed (shift amount)
                            if op.inputs.len() >= 2 {
                                if op.inputs[1].is_constant {
                                    let shift_amt = op.inputs[1].constant_val as u32;
                                    if self
                                        .add_consume(&op.inputs[0], input_consume_req >> shift_amt)
                                    {
                                        changed = true;
                                    }
                                } else {
                                    // Unknown shift, assume B is fully consumed
                                    if self.add_consume(&op.inputs[0], u64::MAX) {
                                        changed = true;
                                    }
                                }
                                // Shift amount is fully consumed
                                if self.add_consume(&op.inputs[1], u64::MAX) {
                                    changed = true;
                                }
                            }
                        }

                        PcodeOpcode::IntRight | PcodeOpcode::IntSRight => {
                            // A = B >> C
                            // Bits of B consumed = (Bits of A consumed) << C
                            if op.inputs.len() >= 2 {
                                if op.inputs[1].is_constant {
                                    let shift_amt = op.inputs[1].constant_val as u32;
                                    if self
                                        .add_consume(&op.inputs[0], input_consume_req << shift_amt)
                                    {
                                        changed = true;
                                    }
                                } else {
                                    if self.add_consume(&op.inputs[0], u64::MAX) {
                                        changed = true;
                                    }
                                }
                                if self.add_consume(&op.inputs[1], u64::MAX) {
                                    changed = true;
                                }
                            }
                        }

                        PcodeOpcode::SubPiece => {
                            // A = Subpiece(B, offset)
                            // Consumes bits of B at [offset..offset+size]
                            if op.inputs.len() >= 2 {
                                let offset = if op.inputs[1].is_constant {
                                    op.inputs[1].constant_val as u32
                                } else {
                                    0
                                };
                                // Create mask for the slice in B's space
                                // The consumed bits of A (input_consume_req) map to B at (req << (offset*8))
                                let shifted_req = input_consume_req << (offset * 8);
                                if self.add_consume(&op.inputs[0], shifted_req) {
                                    changed = true;
                                }
                            }
                        }

                        PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                            // A = ZExt(B)
                            // B is consumed where A is consumed (low bits)
                            if !op.inputs.is_empty() {
                                // Mask input_consume_req to B's size
                                let b_size = op.inputs[0].size;
                                let b_mask = self.size_mask(b_size);
                                if self.add_consume(&op.inputs[0], input_consume_req & b_mask) {
                                    changed = true;
                                }
                            }
                        }

                        _ => {
                            // Default: All inputs fully consumed (conservative)
                            // Or propagate full output requirement?
                            // For arithmetic (Add, Sub, Mult), bits propagate complexly.
                            // E.g. Add: Low bits affect high bits (carry).
                            // So if bit N of output is consumed, bits 0..N of inputs are consumed.
                            // For now, let's be conservative and say if ANY output bit is consumed,
                            // ALL input bits are consumed.
                            // UNLESS output is completely unused (input_consume_req == 0).

                            let req = if input_consume_req == 0 { 0 } else { u64::MAX };
                            for input in &op.inputs {
                                if self.add_consume(input, req) {
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    /// Helper to add consume bits to a varnode
    fn add_consume(&mut self, vn: &Varnode, mask: u64) -> bool {
        if vn.is_constant {
            return false;
        }
        let vn_id = VarnodeId::from(vn);
        if let Some(info) = self.def_use.get_mut(&vn_id) {
            let old = info.consume_mask;
            let new = old | mask;
            if new != old {
                info.consume_mask = new;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::{PcodeBasicBlock, PcodeOp, PcodeOpcode, Varnode};

    #[test]
    fn test_def_use_tracking() {
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1000,
                        output: Some(Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }),
                        inputs: vec![Varnode::constant(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1000,
                        output: Some(Varnode {
                            space_id: 1,
                            offset: 0x200,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }),
                        inputs: vec![
                            Varnode {
                                space_id: 1,
                                offset: 0x100,
                                size: 4,
                                is_constant: false,
                                constant_val: 0,
                            },
                            Varnode::constant(3, 4),
                        ],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let mut tracker = DefUseTracker::new();
        tracker.build(&func);

        let Some(v1) = func.blocks[0].ops[0].output.as_ref() else {
            panic!("test setup requires op output")
        };
        assert!(tracker.is_written(v1));
        assert_eq!(tracker.get_uses(v1).len(), 1);
    }

    #[test]
    fn test_nz_mask_and() {
        let vn = Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };

        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(vn.clone()),
                    inputs: vec![Varnode::constant(0x0F, 4), Varnode::constant(0xFF, 4)],
                    asm_mnemonic: None,
                }],
            }],
        };

        let mut tracker = DefUseTracker::new();
        tracker.build(&func);

        let mask = tracker.get_nz_mask(&vn);
        assert_eq!(mask, 0x0F);
    }
}
