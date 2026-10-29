//! Dead code elimination for Pcode operations
//!
//! Removes operations whose outputs are never used, except for operations
//! with side effects (stores, calls, returns, branches).

use crate::pcode::{PcodeFunction, PcodeOpcode};
use std::collections::HashSet;

/// Dead code eliminator
pub struct DeadCodeEliminator {
    // Can add configuration or statistics here later
}

impl DeadCodeEliminator {
    pub fn new() -> Self {
        Self {}
    }

    /// Eliminate dead code (operations with unused outputs)
    pub fn eliminate(&self, func: &mut PcodeFunction, modified: &mut bool) {
        // Build use-def chains
        let mut used_varnodes = HashSet::new();

        // Mark all used varnodes
        for block in &func.blocks {
            for op in &block.ops {
                // Control flow ops implicitly use their inputs
                if matches!(
                    op.opcode,
                    PcodeOpcode::CBranch
                        | PcodeOpcode::Return
                        | PcodeOpcode::Store
                        | PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                ) {
                    for input in &op.inputs {
                        used_varnodes.insert(input.clone());
                    }
                }

                // All inputs are used
                for input in &op.inputs {
                    used_varnodes.insert(input.clone());
                }
            }
        }

        // Remove operations with unused outputs
        for block in &mut func.blocks {
            let original_len = block.ops.len();

            block.ops.retain(|op| {
                // Keep operations with side effects
                if matches!(
                    op.opcode,
                    PcodeOpcode::Store
                        | PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::Return
                        | PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                ) {
                    return true;
                }

                // Keep if output is used
                if let Some(out) = &op.output {
                    return used_varnodes.contains(out);
                }

                true
            });

            if block.ops.len() < original_len {
                *modified = true;
            }
        }
    }
}
