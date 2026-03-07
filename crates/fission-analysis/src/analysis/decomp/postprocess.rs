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
pub mod pass;
pub mod passes;
pub mod registry;
mod structure;
mod switch_recon;
#[cfg(test)]
mod tests;

/// Configurable options for the Rust-side post-processing passes.
///
/// Each flag corresponds to one pass in [`PostProcessor::process`].
/// All default to `true` (enabled).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RustPostProcessOptions {
    pub clean_rust: bool,
    pub clean_go: bool,
    pub swift_demangle: bool,
    pub field_offsets: bool,
    pub insert_casts: bool,
    pub arithmetic_idioms: bool,
    pub deref_to_array: bool,
    pub bitop_to_logicop: bool,
    pub remove_dead_branches: bool,
    pub simplify_if: bool,
    pub while_to_for: bool,
    pub dead_assign_removal: bool,
    pub rename_induction_vars: bool,
    pub rename_semantic_vars: bool,
    pub loop_idioms: bool,
    pub switch_reconstruction: bool,
    pub mul_to_shift: bool,
    pub dwarf_names: bool,
}

impl Default for RustPostProcessOptions {
    fn default() -> Self {
        Self {
            clean_rust: true,
            clean_go: true,
            swift_demangle: true,
            field_offsets: true,
            insert_casts: true,
            arithmetic_idioms: true,
            deref_to_array: true,
            bitop_to_logicop: true,
            remove_dead_branches: true,
            simplify_if: true,
            while_to_for: true,
            dead_assign_removal: true,
            rename_induction_vars: true,
            rename_semantic_vars: true,
            loop_idioms: true,
            switch_reconstruction: true,
            mul_to_shift: true,
            dwarf_names: true,
        }
    }
}

/// Decompiler output post-processor
pub struct PostProcessor {
    options: RustPostProcessOptions,
    inferred_types: Vec<InferredTypeInfo>,
    dwarf_info: Option<DwarfFunctionInfo>,
}

impl PostProcessor {
    pub fn new() -> Self {
        Self {
            options: RustPostProcessOptions::default(),
            inferred_types: Vec::new(),
            dwarf_info: None,
        }
    }

    /// Configure post-processing passes via options struct
    pub fn with_options(mut self, options: RustPostProcessOptions) -> Self {
        self.options = options;
        self
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

    /// Process using the new trait-based pass system
    ///
    /// This is the recommended method - it uses the PassRegistry
    /// for dynamic pass management and automatic dependency resolution.
    pub fn process_with_registry(&self, code: &str) -> Result<String, String> {
        use pass::PassContext;

        // Create a pass context with our configuration
        let mut context = PassContext::new();

        // Add type information if available
        if !self.inferred_types.is_empty() {
            context.inferred_types = self.inferred_types.clone();
        }

        // Add DWARF info if available
        if let Some(ref dwarf) = self.dwarf_info {
            context.dwarf_info = Some(dwarf.clone());
        }

        // Create a registry with all passes
        let mut pass_registry = registry::create_default_registry()?;

        // Disable passes based on options
        if !self.options.clean_rust {
            pass_registry.disable("remove_rust_boilerplate");
        }
        if !self.options.clean_go {
            pass_registry.disable("remove_go_boilerplate");
        }
        if !self.options.swift_demangle {
            pass_registry.disable("swift_demangle");
        }
        if !self.options.field_offsets {
            pass_registry.disable("field_offset_replacement");
        }
        if !self.options.insert_casts {
            pass_registry.disable("insert_missing_casts");
        }
        if !self.options.arithmetic_idioms {
            pass_registry.disable("arithmetic_idioms");
        }
        if !self.options.deref_to_array {
            pass_registry.disable("deref_to_array_index");
        }
        if !self.options.bitop_to_logicop {
            pass_registry.disable("bitop_to_logicop");
        }
        if !self.options.remove_dead_branches {
            pass_registry.disable("remove_constant_conditions");
        }
        if !self.options.simplify_if {
            pass_registry.disable("simplify_if_structure");
        }
        if !self.options.while_to_for {
            pass_registry.disable("while_true_to_cond");
            pass_registry.disable("while_true_to_for");
            pass_registry.disable("while_cond_to_for");
            pass_registry.disable("do_while_to_for");
            pass_registry.disable("while_true_to_for_ever");
        }
        if !self.options.dead_assign_removal {
            pass_registry.disable("remove_dead_assignments");
        }
        if !self.options.rename_induction_vars {
            pass_registry.disable("rename_induction_vars");
        }
        if !self.options.rename_semantic_vars {
            pass_registry.disable("rename_semantic_vars");
        }
        if !self.options.loop_idioms {
            pass_registry.disable("loop_idioms");
        }
        if !self.options.switch_reconstruction {
            pass_registry.disable("switch_reconstruction");
            pass_registry.disable("switch_from_if_else_assign");
        }
        if !self.options.mul_to_shift {
            pass_registry.disable("mul_pow2_to_shift");
        }
        if !self.options.dwarf_names {
            pass_registry.disable("apply_dwarf_names");
        }

        // Execute all enabled passes with dependency resolution
        pass_registry
            .execute_all(code, &context)
            .map(|output| output.into_owned())
            .map_err(|e| e.to_string())
    }

    /// Process the decompiler output to remove boilerplate (legacy method)
    ///
    /// This method uses the original direct method calling approach.
    /// For new code, prefer [`process_with_registry`] which provides
    /// better dependency management and extensibility.
    pub fn process(&self, code: &str) -> String {
        let mut processed = code.to_string();

        if self.options.clean_rust {
            processed = self.remove_rust_boilerplate(&processed);
        }

        if self.options.clean_go {
            processed = self.remove_go_boilerplate(&processed);
        }

        // Demangle Swift symbols
        if self.options.swift_demangle {
            processed = self.demangle_swift_symbols(&processed);
        }

        // Apply field offset replacement if we have type info
        if self.options.field_offsets && !self.inferred_types.is_empty() {
            processed = self.replace_field_offsets(&processed);
        }

        // Insert missing casts for assignment type mismatches
        if self.options.insert_casts {
            processed = Self::insert_missing_casts(&processed);
        }

        // Apply arithmetic idiom recovery
        if self.options.arithmetic_idioms {
            processed = self.apply_arithmetic_idioms(&processed);
        }

        // =====================================================================
        // Phase A: RetDec-inspired post-processing passes
        // Order follows RetDec's optimizer_manager.cpp —
        //   expressions → structure → dead code → naming
        // =====================================================================

        // A-1: Deref → Array index: *(a + N) → a[N]
        if self.options.deref_to_array {
            processed = Self::deref_to_array_index(&processed);
        }

        // A-2: Bit-op → Logical-op in conditions: (cmp1) & (cmp2) → cmp1 && cmp2
        if self.options.bitop_to_logicop {
            processed = Self::bitop_to_logicop(&processed);
        }

        // A-3: Constant condition / dead branch removal
        if self.options.remove_dead_branches {
            processed = Self::remove_constant_conditions(&processed);
        }

        // A-4: Empty else removal + If-return early exit
        if self.options.simplify_if {
            processed = Self::simplify_if_structure(&processed);
        }

        // A-5: while(true) { if(c) break; S } → while(!c) { S }
        if self.options.while_to_for {
            processed = Self::while_true_to_while_cond(&processed);
        }

        // =====================================================================
        // Phase B: Advanced structural + naming passes
        // =====================================================================

        // B-1: while(true) → for loop (init + exit-cond + update detection)
        if self.options.while_to_for {
            processed = Self::while_true_to_for_loop(&processed);
        }

        // B-2: Dead local assignment removal (2 iterations for cascading)
        if self.options.dead_assign_removal {
            processed = Self::remove_dead_local_assigns(&processed);
            processed = Self::remove_dead_local_assigns(&processed);
        }

        // B-3: Induction variable naming (i, j, k for loop counters)
        if self.options.rename_induction_vars {
            processed = Self::rename_induction_vars(&processed);
        }

        // B-4: Semantic variable naming (main→argc/argv, return→result, API results)
        if self.options.rename_semantic_vars {
            processed = Self::rename_semantic_vars(&processed);
        }

        // B-5: Loop idiom recognition (strlen, popcount, memset)
        if self.options.loop_idioms {
            processed = Self::recognize_loop_idioms(&processed);
        }

        // Reconstruct switch from BST / sequential equality-return patterns
        if self.options.switch_reconstruction {
            processed = Self::reconstruct_switch_from_bst(&processed);
        }

        // B-6: Reconstruct switch from if/else-if assignment chains
        // e.g.: if (!x) { r = A; } else if (x == 1) { r = B; } ... return r;
        if self.options.switch_reconstruction {
            processed = Self::reconstruct_switch_from_if_else_assign(&processed);
        }

        // B-7: General while(cond) → for conversion when init+increment detected
        if self.options.while_to_for {
            processed = Self::while_cond_to_for(&processed);
        }

        // B-8: do { ... VAR++; } while (VAR op LIMIT); → for (...)
        if self.options.while_to_for {
            processed = Self::do_while_to_for(&processed);
        }

        // B-9: Multiply by power-of-2 → bitshift  (e.g. * 256 → << 8)
        if self.options.mul_to_shift {
            processed = Self::mul_pow2_to_shift(&processed);
        }

        // B-10: while( true ) / while(true) → for (;;)
        if self.options.while_to_for {
            processed = Self::while_true_to_for_ever(&processed);
        }

        // Apply DWARF variable/parameter name substitution
        if self.options.dwarf_names && self.dwarf_info.is_some() {
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
