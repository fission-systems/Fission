//! Pcode optimizer - applies optimization rules directly to Pcode IR
//!
//! This is more accurate than string-based C code optimization because:
//! 1. No parsing ambiguity
//! 2. Full type information preserved
//! 3. Can optimize before high-level C constructs are generated
//! 4. Matches Ghidra's own optimization framework

mod cse;
mod dead_code;
mod def_use;
mod rules;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dead_bit;

use crate::pcode::{PcodeFunction, PcodeOpcode};

pub use cse::CommonSubexpressionEliminator;
pub use dead_code::DeadCodeEliminator;
pub use def_use::DefUseTracker;
pub use rules::OptimizationRules;

/// Configuration for Pcode optimization
#[derive(Debug, Clone)]
pub struct PcodeOptimizerConfig {
    pub enable_constant_folding: bool,
    pub enable_identity_removal: bool,
    pub enable_algebraic_simplification: bool,
    pub enable_dead_code_elimination: bool,
    pub enable_cse: bool,
}

impl Default for PcodeOptimizerConfig {
    fn default() -> Self {
        Self {
            enable_constant_folding: true,
            enable_identity_removal: true,
            enable_algebraic_simplification: true,
            enable_dead_code_elimination: true,
            enable_cse: true,
        }
    }
}

/// Main Pcode optimizer
pub struct PcodeOptimizer {
    config: PcodeOptimizerConfig,
    modified: bool,
    rules: OptimizationRules,
    dead_code_eliminator: DeadCodeEliminator,
    def_use_tracker: DefUseTracker,
    cse: CommonSubexpressionEliminator,
}

impl PcodeOptimizer {
    pub fn new(config: PcodeOptimizerConfig) -> Self {
        Self {
            config,
            modified: false,
            rules: OptimizationRules::new(),
            def_use_tracker: DefUseTracker::new(),
            dead_code_eliminator: DeadCodeEliminator::new(),
            cse: CommonSubexpressionEliminator::new(),
        }
    }

    /// Optimize a Pcode function (may run multiple passes)
    pub fn optimize(&mut self, func: &mut PcodeFunction) -> usize {
        let mut total_changes = 0;
        let max_passes = 10;

        for pass in 0..max_passes {
            self.modified = false;

            // Build def-use chains and compute NZ masks
            self.def_use_tracker.build(func);

            // Pass 1: Constant folding & algebraic simplification
            if self.config.enable_constant_folding || self.config.enable_algebraic_simplification {
                self.optimize_arithmetic(func);
            }

            // Pass 2: Common Subexpression Elimination (CSE)
            if self.config.enable_cse {
                if self.cse.eliminate(func) {
                    self.modified = true;
                }
            }

            // Pass 3: Identity operation removal
            if self.config.enable_identity_removal {
                self.remove_identity_ops(func);
            }

            // Pass 4: Dead code elimination
            if self.config.enable_dead_code_elimination {
                self.dead_code_eliminator
                    .eliminate(func, &mut self.modified);
            }

            if !self.modified {
                eprintln!(
                    "[PcodeOptimizer] Converged after {} passes ({} total changes)",
                    pass + 1,
                    total_changes
                );
                break;
            }

            total_changes += 1;
        }

        total_changes
    }

    /// Optimize arithmetic operations
    fn optimize_arithmetic(&mut self, func: &mut PcodeFunction) {
        let mut modifications = Vec::new();

        for (block_idx, block) in func.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                // Use try_optimize_with_tracker which includes all rules
                if let Some(optimized) =
                    self.rules
                        .try_optimize_with_tracker(op, &self.def_use_tracker, func)
                {
                    modifications.push((block_idx, op_idx, optimized));
                }
            }
        }

        if !modifications.is_empty() {
            self.modified = true;
            for (block_idx, op_idx, new_op) in modifications {
                func.blocks[block_idx].ops[op_idx] = new_op;
            }
        }
    }

    /// Remove identity operations (COPY x -> x, etc.)
    fn remove_identity_ops(&mut self, func: &mut PcodeFunction) {
        for block in &mut func.blocks {
            let original_len = block.ops.len();

            block.ops.retain(|op| {
                // Remove COPY where output == input
                if op.opcode == PcodeOpcode::Copy && op.inputs.len() == 1 {
                    if let Some(out) = &op.output {
                        if &op.inputs[0] == out {
                            return false; // Remove this op
                        }
                    }
                }
                true
            });

            if block.ops.len() < original_len {
                self.modified = true;
            }
        }
    }
}
