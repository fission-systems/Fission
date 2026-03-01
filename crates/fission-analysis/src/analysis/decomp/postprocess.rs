//! Decompiler Post-Processor
//!
//! Provides IDA-style code cleaning and boilerplate removal.
//!
//! This module processes raw C code from the decompiler to make it more
//! readable by hiding language-specific overhead like safety checks and panics.

use fission_loader::loader::types::{DwarfFunctionInfo, InferredTypeInfo};

mod arithmetic;
mod cleanup;
mod condition;
mod loops;
mod naming;
mod structure;
mod switch_recon;
#[cfg(test)]
mod tests;

/// Decompiler output post-processor
pub struct PostProcessor {
    clean_rust: bool,
    clean_go: bool,
    inferred_types: Vec<InferredTypeInfo>,
    dwarf_info: Option<DwarfFunctionInfo>,
}

impl PostProcessor {
    pub fn new() -> Self {
        Self {
            clean_rust: true,
            clean_go: true,
            inferred_types: Vec::new(),
            dwarf_info: None,
        }
    }

    /// Set inferred types for field name resolution
    pub fn with_inferred_types(mut self, types: Vec<InferredTypeInfo>) -> Self {
        self.inferred_types = types;
        self
    }

    /// Set DWARF function info for variable/parameter name substitution
    pub fn with_dwarf_info(mut self, info: Option<DwarfFunctionInfo>) -> Self {
        self.dwarf_info = info;
        self
    }

    /// Process the decompiler output to remove boilerplate
    pub fn process(&self, code: &str) -> String {
        let mut processed = code.to_string();

        if self.clean_rust {
            processed = self.remove_rust_boilerplate(&processed);
        }

        if self.clean_go {
            processed = self.remove_go_boilerplate(&processed);
        }

        // Always attempt to demangle Swift symbols
        processed = self.demangle_swift_symbols(&processed);

        // Apply field offset replacement if we have type info
        if !self.inferred_types.is_empty() {
            processed = self.replace_field_offsets(&processed);
        }

        // Insert missing casts for assignment type mismatches
        processed = Self::insert_missing_casts(&processed);

        // Apply arithmetic idiom recovery
        processed = self.apply_arithmetic_idioms(&processed);

        // =====================================================================
        // Phase A: RetDec-inspired post-processing passes
        // Order follows RetDec's optimizer_manager.cpp —
        //   expressions → structure → dead code → naming
        // =====================================================================

        // A-1: Deref → Array index: *(a + N) → a[N]
        processed = Self::deref_to_array_index(&processed);

        // A-2: Bit-op → Logical-op in conditions: (cmp1) & (cmp2) → cmp1 && cmp2
        processed = Self::bitop_to_logicop(&processed);

        // A-3: Constant condition / dead branch removal
        processed = Self::remove_constant_conditions(&processed);

        // A-4: Empty else removal + If-return early exit
        processed = Self::simplify_if_structure(&processed);

        // A-5: while(true) { if(c) break; S } → while(!c) { S }
        processed = Self::while_true_to_while_cond(&processed);

        // =====================================================================
        // Phase B: Advanced structural + naming passes
        // =====================================================================

        // B-1: while(true) → for loop (init + exit-cond + update detection)
        processed = Self::while_true_to_for_loop(&processed);

        // B-2: Dead local assignment removal (2 iterations for cascading)
        processed = Self::remove_dead_local_assigns(&processed);
        processed = Self::remove_dead_local_assigns(&processed);

        // B-3: Induction variable naming (i, j, k for loop counters)
        processed = Self::rename_induction_vars(&processed);

        // B-4: Semantic variable naming (main→argc/argv, return→result, API results)
        processed = Self::rename_semantic_vars(&processed);

        // B-5: Loop idiom recognition (strlen, popcount, memset)
        processed = Self::recognize_loop_idioms(&processed);

        // Reconstruct switch from BST / sequential equality-return patterns
        processed = Self::reconstruct_switch_from_bst(&processed);

        // B-6: Reconstruct switch from if/else-if assignment chains
        // e.g.: if (!x) { r = A; } else if (x == 1) { r = B; } ... return r;
        processed = Self::reconstruct_switch_from_if_else_assign(&processed);

        // B-7: General while(cond) → for conversion when init+increment detected
        processed = Self::while_cond_to_for(&processed);

        // B-8: do { ... VAR++; } while (VAR op LIMIT); → for (...)
        processed = Self::do_while_to_for(&processed);

        // B-9: Multiply by power-of-2 → bitshift  (e.g. * 256 → << 8)
        processed = Self::mul_pow2_to_shift(&processed);

        // B-10: while( true ) / while(true) → for (;;)
        processed = Self::while_true_to_for_ever(&processed);

        // Apply DWARF variable/parameter name substitution
        if self.dwarf_info.is_some() {
            processed = self.apply_dwarf_names(&processed);
        }

        processed
    }

}
impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}
