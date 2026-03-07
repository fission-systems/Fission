//! Concrete implementations of post-processing passes
//!
//! This module provides trait implementations for all the existing
//! post-processing passes, wrapping the methods from PostProcessor.

use super::PostProcessor;
use super::pass::{PassCategory, PassContext, PassMetadata, PassResult, PostProcessPass};
use std::borrow::Cow;

fn pass_output<'a>(input: &'a str, output: String) -> PassResult<'a> {
    if output == input {
        Ok(Cow::Borrowed(input))
    } else {
        Ok(Cow::Owned(output))
    }
}

// ============================================================================
// Arithmetic Passes
// ============================================================================

/// Arithmetic idiom recovery pass
///
/// Simplifies compiler-generated bit-twiddling patterns into cleaner arithmetic.
pub struct ArithmeticIdiomsPass;

impl PostProcessPass for ArithmeticIdiomsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "arithmetic_idioms",
            name: "Arithmetic Idiom Recovery",
            description: "Simplifies sign extension, division, CONCAT operations",
            category: PassCategory::Arithmetic,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        // Create a temporary PostProcessor to access the method
        let processor = PostProcessor::new();
        Ok(processor.apply_arithmetic_idioms_cow(code))
    }
}

/// Multiply by power-of-2 to shift pass
pub struct MulPow2ToShiftPass;

impl PostProcessPass for MulPow2ToShiftPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "mul_to_shift",
            name: "Multiply to Shift",
            description: "Converts multiplication by power-of-2 to left shift in bitwise context",
            category: PassCategory::Arithmetic,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        pass_output(code, PostProcessor::mul_pow2_to_shift(code))
    }
}

// ============================================================================
// Control Flow Passes
// ============================================================================

/// While-true to while-condition pass
pub struct WhileTrueToCondPass;

impl PostProcessPass for WhileTrueToCondPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "while_true_to_cond",
            name: "While-True to While-Condition",
            description: "Converts while(true) with break to while(condition)",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::while_true_to_while_cond_cow(code))
    }
}

/// While-true to for-loop pass
pub struct WhileTrueToForPass;

impl PostProcessPass for WhileTrueToForPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "while_true_to_for",
            name: "While-True to For-Loop",
            description: "Converts while(true) with init/exit/update to for loop",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::while_true_to_for_loop_cow(code))
    }

    fn dependencies(&self) -> &[&'static str] {
        &["while_true_to_cond"] // Should run after basic while simplification
    }
}

/// While-condition to for-loop pass
pub struct WhileCondToForPass;

impl PostProcessPass for WhileCondToForPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "while_cond_to_for",
            name: "While-Condition to For-Loop",
            description: "Converts while(cond) to for loop when init/increment detected",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::while_cond_to_for_cow(code))
    }
}

/// Do-while to for-loop pass
pub struct DoWhileToForPass;

impl PostProcessPass for DoWhileToForPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "do_while_to_for",
            name: "Do-While to For-Loop",
            description: "Converts do-while to for loop with increment",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::do_while_to_for_cow(code))
    }
}

/// While-true to for-ever pass
pub struct WhileTrueToForEverPass;

impl PostProcessPass for WhileTrueToForEverPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "while_true_to_for_ever",
            name: "While-True to For-Ever",
            description: "Converts while(true) to for(;;)",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::while_true_to_for_ever_cow(code))
    }

    fn dependencies(&self) -> &[&'static str] {
        &["while_true_to_for"] // Should run after for-loop conversion attempts
    }
}

/// If-structure simplification pass
pub struct SimplifyIfStructurePass;

impl PostProcessPass for SimplifyIfStructurePass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "simplify_if",
            name: "Simplify If-Structure",
            description: "Removes empty else blocks and simplifies if-return patterns",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::simplify_if_structure_cow(code))
    }
}

/// Switch reconstruction from BST pass
pub struct SwitchReconstructionPass;

impl PostProcessPass for SwitchReconstructionPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "switch_from_bst",
            name: "Switch Reconstruction (BST)",
            description: "Reconstructs switch from binary search tree patterns",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::reconstruct_switch_from_bst_cow(code))
    }
}

/// Switch reconstruction from if-else-assign pass
pub struct SwitchFromIfElseAssignPass;

impl PostProcessPass for SwitchFromIfElseAssignPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "switch_from_if_else",
            name: "Switch Reconstruction (If-Else)",
            description: "Reconstructs switch from if-else assignment chains",
            category: PassCategory::ControlFlow,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::reconstruct_switch_from_if_else_assign_cow(
            code,
        ))
    }

    fn dependencies(&self) -> &[&'static str] {
        &["switch_from_bst"] // Run after BST reconstruction
    }
}

// ============================================================================
// Cleanup Passes
// ============================================================================

/// Constant condition removal pass
pub struct RemoveConstantConditionsPass;

impl PostProcessPass for RemoveConstantConditionsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "remove_dead_branches",
            name: "Remove Constant Conditions",
            description: "Removes dead branches with constant conditions",
            category: PassCategory::Cleanup,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::remove_constant_conditions_cow(code))
    }
}

/// Dead local assignment removal pass
pub struct RemoveDeadAssignmentsPass {
    /// Number of iterations to run (for cascading removal)
    iterations: usize,
}

impl RemoveDeadAssignmentsPass {
    pub fn new(iterations: usize) -> Self {
        Self { iterations }
    }
}

impl PostProcessPass for RemoveDeadAssignmentsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "remove_dead_assigns",
            name: "Remove Dead Assignments",
            description: "Removes unused local variable assignments",
            category: PassCategory::Cleanup,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        if self.iterations == 0 {
            return Ok(Cow::Borrowed(code));
        }

        let first = PostProcessor::remove_dead_local_assigns_cow(code);
        if self.iterations == 1 {
            return Ok(first);
        }

        let mut owned = first.into_owned();
        for _ in 1..self.iterations {
            owned = PostProcessor::remove_dead_local_assigns_cow(&owned).into_owned();
        }
        pass_output(code, owned)
    }
}

/// Array index conversion pass
pub struct DerefToArrayIndexPass;

impl PostProcessPass for DerefToArrayIndexPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "deref_to_array",
            name: "Deref to Array Index",
            description: "Converts *(a + N) to a[N]",
            category: PassCategory::Cleanup,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::deref_to_array_index_cow(code))
    }
}

/// Bitwise to logical operator pass
pub struct BitopToLogicopPass;

impl PostProcessPass for BitopToLogicopPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "bitop_to_logicop",
            name: "Bitwise to Logical Operators",
            description: "Converts bitwise operators to logical in conditions",
            category: PassCategory::Cleanup,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::bitop_to_logicop_cow(code))
    }
}

// ============================================================================
// Naming Passes
// ============================================================================

/// Induction variable renaming pass
pub struct RenameInductionVarsPass;

impl PostProcessPass for RenameInductionVarsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "rename_induction_vars",
            name: "Rename Induction Variables",
            description: "Renames loop counters to i, j, k",
            category: PassCategory::Naming,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::rename_induction_vars_cow(code))
    }

    fn dependencies(&self) -> &[&'static str] {
        &["while_true_to_for", "while_cond_to_for"] // Run after loop conversion
    }
}

/// Semantic variable renaming pass
pub struct RenameSemanticVarsPass;

impl PostProcessPass for RenameSemanticVarsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "rename_semantic_vars",
            name: "Rename Semantic Variables",
            description: "Renames variables based on semantic context (argc/argv, result, etc.)",
            category: PassCategory::Naming,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::rename_semantic_vars_cow(code))
    }
}

/// Loop idiom recognition pass
pub struct LoopIdiomsPass;

impl PostProcessPass for LoopIdiomsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "loop_idioms",
            name: "Loop Idiom Recognition",
            description: "Recognizes common loop patterns (strlen, popcount, memset)",
            category: PassCategory::Naming,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::recognize_loop_idioms_cow(code))
    }

    fn dependencies(&self) -> &[&'static str] {
        &["while_true_to_for"] // Run after loop structure is clear
    }
}

// ============================================================================
// Language-Specific Passes
// ============================================================================

/// Rust boilerplate removal pass
pub struct RemoveRustBoilerplatePass;

impl PostProcessPass for RemoveRustBoilerplatePass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "clean_rust",
            name: "Remove Rust Boilerplate",
            description: "Removes Rust-specific panic and safety checks",
            category: PassCategory::LanguageSpecific,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        let processor = PostProcessor::new();
        Ok(processor.remove_rust_boilerplate_cow(code))
    }
}

/// Go boilerplate removal pass
pub struct RemoveGoBoilerplatePass;

impl PostProcessPass for RemoveGoBoilerplatePass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "clean_go",
            name: "Remove Go Boilerplate",
            description: "Removes Go-specific panic patterns",
            category: PassCategory::LanguageSpecific,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        let processor = PostProcessor::new();
        Ok(processor.remove_go_boilerplate_cow(code))
    }
}

/// Swift symbol demangling pass
pub struct SwiftDemanglePass;

impl PostProcessPass for SwiftDemanglePass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "swift_demangle",
            name: "Swift Symbol Demangling",
            description: "Demangles Swift symbols",
            category: PassCategory::LanguageSpecific,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        let processor = PostProcessor::new();
        Ok(processor.demangle_swift_symbols_cow(code))
    }
}

// ============================================================================
// Type-Based Passes
// ============================================================================

/// Field offset replacement pass
pub struct FieldOffsetReplacementPass;

impl PostProcessPass for FieldOffsetReplacementPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "field_offsets",
            name: "Field Offset Replacement",
            description: "Replaces numeric offsets with field names using type info",
            category: PassCategory::TypeBased,
        }
    }

    fn run<'a>(&self, code: &'a str, context: &PassContext) -> PassResult<'a> {
        if context.inferred_types.is_empty() {
            return Ok(Cow::Borrowed(code));
        }

        let processor = PostProcessor::new().with_inferred_types(context.inferred_types.clone());
        Ok(processor.replace_field_offsets_cow(code))
    }

    fn should_run(&self, context: &PassContext) -> bool {
        !context.inferred_types.is_empty()
    }
}

/// Missing cast insertion pass
pub struct InsertMissingCastsPass;

impl PostProcessPass for InsertMissingCastsPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "insert_casts",
            name: "Insert Missing Casts",
            description: "Inserts missing type casts for assignments",
            category: PassCategory::TypeBased,
        }
    }

    fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
        Ok(PostProcessor::insert_missing_casts_cow(code))
    }
}

/// DWARF name application pass
pub struct ApplyDwarfNamesPass;

impl PostProcessPass for ApplyDwarfNamesPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "dwarf_names",
            name: "Apply DWARF Names",
            description: "Substitutes variable/parameter names from DWARF debug info",
            category: PassCategory::TypeBased,
        }
    }

    fn run<'a>(&self, code: &'a str, context: &PassContext) -> PassResult<'a> {
        if context.dwarf_info.is_none() {
            return Ok(Cow::Borrowed(code));
        }

        let processor = PostProcessor::new().with_dwarf_info(context.dwarf_info.clone());
        Ok(processor.apply_dwarf_names_cow(code))
    }

    fn should_run(&self, context: &PassContext) -> bool {
        context.dwarf_info.is_some()
    }
}
