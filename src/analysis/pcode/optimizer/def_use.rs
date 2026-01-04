/// Def-use chain tracking and analysis for Pcode optimization
/// 
/// This module provides infrastructure for:
/// - Tracking which operations define and use each varnode
/// - Computing non-zero masks (NZMask) for values
/// - Enabling advanced optimizations like CSE

use crate::analysis::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::collections::HashMap;

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
                    
                    self.def_use.entry(vn_id).or_insert_with(|| DefUseInfo {
                        def: Some(op_ref),
                        uses: Vec::new(),
                        nz_mask: 0,
                        consume_mask: 0,
                    }).def = Some(op_ref);
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
                        self.def_use.entry(vn_id).or_insert_with(|| DefUseInfo {
                            def: None,
                            uses: Vec::new(),
                            nz_mask: 0,
                            consume_mask: 0,
                        }).uses.push(op_ref);
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
        self.get_info(vn).map(|info| info.uses.clone()).unwrap_or_default()
    }
    
    /// Get non-zero mask for a varnode
    pub fn get_nz_mask(&self, vn: &Varnode) -> u64 {
        if vn.is_constant {
            return self.constant_nz_mask(vn);
        }
        self.get_info(vn).map(|info| info.nz_mask).unwrap_or(u64::MAX)
    }
    
    /// Get consume mask for a varnode
    pub fn get_consume_mask(&self, vn: &Varnode) -> u64 {
        if vn.is_constant {
            return u64::MAX; // Constants are always fully consumed
        }
        self.get_info(vn).map(|info| info.consume_mask).unwrap_or(u64::MAX)
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
        let out_size = op.output.as_ref().map(|v| v.size).unwrap_or(4);
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
        // Start with all output varnodes having full consume mask
        let vn_ids: Vec<VarnodeId> = self.def_use.keys().cloned().collect();
        for vn_id in vn_ids {
            if let Some(info) = self.def_use.get_mut(&vn_id) {
                info.consume_mask = u64::MAX;
            }
        }
        
        // Propagate backwards from uses
        // This is a simplified version; full implementation would iterate to convergence
        for block in &func.blocks {
            for op in &block.ops {
                // Operations that care about specific bits
                match op.opcode {
                    PcodeOpcode::IntAnd => {
                        if op.inputs.len() >= 2 {
                            // AND propagates consume mask through both inputs
                            let out_consume = if let Some(out) = &op.output {
                                self.get_consume_mask(out)
                            } else {
                                u64::MAX
                            };
                            
                            // Each input only needs bits that will affect output
                            for input in &op.inputs {
                                if !input.is_constant {
                                    let vn_id = VarnodeId::from(input);
                                    if let Some(info) = self.def_use.get_mut(&vn_id) {
                                        info.consume_mask &= out_consume;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pcode::{PcodeBasicBlock, PcodeOp, PcodeOpcode, Varnode};
    
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
                    },
                ],
            }],
        };
        
        let mut tracker = DefUseTracker::new();
        tracker.build(&func);
        
        let v1 = &func.blocks[0].ops[0].output.as_ref().unwrap();
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
                }],
            }],
        };
        
        let mut tracker = DefUseTracker::new();
        tracker.build(&func);
        
        let mask = tracker.get_nz_mask(&vn);
        assert_eq!(mask, 0x0F);
    }
}
