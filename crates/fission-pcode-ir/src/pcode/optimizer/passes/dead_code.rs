//! Dead code elimination for Pcode operations
//!
//! Removes operations whose outputs are never used, except for operations
//! with side effects (stores, calls, returns, branches).

use crate::pcode::{PcodeFunction, PcodeOpcode, Varnode};
use std::collections::HashMap;

/// Dead code eliminator
pub struct DeadCodeEliminator {
    // Can add configuration or statistics here later
}

fn is_varnode_used(used_ranges_by_space: &HashMap<u64, Vec<(u64, u64)>>, out: &Varnode) -> bool {
    if out.is_constant {
        return false;
    }
    let Some(ranges) = used_ranges_by_space.get(&out.space_id) else {
        return false;
    };
    let out_start = out.offset;
    let out_end = out.offset.saturating_add(u64::from(out.size.max(1)));
    let idx = ranges.partition_point(|&(start, _)| start < out_end);
    idx > 0 && ranges[idx - 1].1 > out_start
}

impl DeadCodeEliminator {
    pub fn new() -> Self {
        Self {}
    }

    /// Eliminate dead code (operations with unused outputs)
    pub fn eliminate(&self, func: &mut PcodeFunction, modified: &mut bool) {
        // Record half-open `[start, end)` byte intervals per address space so
        // reading a sub-register or super-register preserves the overlapping def.
        let mut used_ranges_by_space: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();

        for block in &func.blocks {
            for op in &block.ops {
                for input in &op.inputs {
                    if !input.is_constant {
                        let end = input.offset.saturating_add(u64::from(input.size.max(1)));
                        used_ranges_by_space
                            .entry(input.space_id)
                            .or_default()
                            .push((input.offset, end));
                    }
                }
            }
        }

        for ranges in used_ranges_by_space.values_mut() {
            ranges.sort_unstable_by_key(|&(start, end)| (start, end));
            let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
            for (start, end) in ranges.drain(..) {
                if let Some((_, prev_end)) = merged.last_mut()
                    && start <= *prev_end
                {
                    *prev_end = (*prev_end).max(end);
                    continue;
                }
                merged.push((start, end));
            }
            *ranges = merged;
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
                        | PcodeOpcode::CallOther
                        | PcodeOpcode::Return
                        | PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                        | PcodeOpcode::BranchInd
                ) {
                    return true;
                }

                // Keep if output overlaps any read byte range
                if let Some(out) = &op.output {
                    return is_varnode_used(&used_ranges_by_space, out);
                }

                true
            });

            if block.ops.len() < original_len {
                *modified = true;
            }
        }
    }
}
