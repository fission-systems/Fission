//! Pass registry builder and default configuration
//!
//! Provides convenient functions to create a fully-configured PassRegistry
//! with all available passes registered.

use super::pass::{PassContext, PassExecutionStats, PassRegistry};
use super::passes::*;

/// Create a PassRegistry with all available passes registered
///
/// All passes are enabled by default. Use the returned registry to
/// selectively disable passes as needed.
pub fn create_default_registry() -> Result<PassRegistry, String> {
    let mut registry = PassRegistry::new();

    // ========================================================================
    // Language-Specific Passes (run first to clean up boilerplate)
    // ========================================================================
    registry.register(Box::new(RemoveRustBoilerplatePass))?;
    registry.register(Box::new(RemoveGoBoilerplatePass))?;
    registry.register(Box::new(SwiftDemanglePass))?;

    // ========================================================================
    // Type-Based Passes (early to enable better analysis)
    // ========================================================================
    registry.register(Box::new(FieldOffsetReplacementPass))?;
    registry.register(Box::new(InsertMissingCastsPass))?;

    // ========================================================================
    // Arithmetic Passes
    // ========================================================================
    registry.register(Box::new(ArithmeticIdiomsPass))?;

    // ========================================================================
    // Expression Cleanup (RetDec Phase A)
    // ========================================================================
    registry.register(Box::new(DerefToArrayIndexPass))?;
    registry.register(Box::new(BitopToLogicopPass))?;

    // ========================================================================
    // Dead Code Elimination
    // ========================================================================
    registry.register(Box::new(RemoveConstantConditionsPass))?;
    registry.register(Box::new(RemoveDeadAssignmentsPass::new(2)))?; // 2 iterations

    // ========================================================================
    // Control Flow Restructuring
    // ========================================================================
    registry.register(Box::new(SimplifyIfStructurePass))?;
    registry.register(Box::new(WhileTrueToCondPass))?;
    registry.register(Box::new(WhileTrueToForPass))?;
    registry.register(Box::new(WhileCondToForPass))?;
    registry.register(Box::new(DoWhileToForPass))?;
    registry.register(Box::new(WhileTrueToForEverPass))?;

    // ========================================================================
    // Switch Reconstruction (after control flow is simplified)
    // ========================================================================
    registry.register(Box::new(SwitchReconstructionPass))?;
    registry.register(Box::new(SwitchFromIfElseAssignPass))?;

    // ========================================================================
    // Naming Passes (after structure is clear)
    // ========================================================================
    registry.register(Box::new(RenameInductionVarsPass))?;
    registry.register(Box::new(RenameSemanticVarsPass))?;
    registry.register(Box::new(LoopIdiomsPass))?;

    // ========================================================================
    // Arithmetic Optimization (late to work on clean code)
    // ========================================================================
    registry.register(Box::new(MulPow2ToShiftPass))?;

    // ========================================================================
    // DWARF Names (last to preserve user intent)
    // ========================================================================
    registry.register(Box::new(ApplyDwarfNamesPass))?;

    Ok(registry)
}

/// Execute all passes using the default configuration
///
/// This is a convenience function that creates a registry, registers all
/// passes, and executes them in dependency order.
pub fn execute_default_passes(code: &str, context: &PassContext) -> Result<String, String> {
    let registry = create_default_registry()?;
    registry
        .execute_all(code, context)
        .map(|output| output.into_owned())
        .map_err(|e| e.to_string())
}

/// Execute all passes using the default configuration and return execution stats.
pub fn execute_default_passes_with_stats(
    code: &str,
    context: &PassContext,
) -> Result<(String, PassExecutionStats), String> {
    let registry = create_default_registry()?;
    registry
        .execute_all_with_stats(code, context)
        .map(|(output, stats)| (output.into_owned(), stats))
        .map_err(|e| e.to_string())
}

/// Create a PassRegistry with only specific categories enabled
pub fn create_registry_for_categories(
    categories: &[super::pass::PassCategory],
) -> Result<PassRegistry, String> {
    let mut registry = create_default_registry()?;

    // Get all registered passes
    let all_passes = registry.list_passes();

    // Disable passes not in the specified categories
    for pass_id in all_passes {
        if let Some(metadata) = registry.get_metadata(&pass_id) {
            if !categories.contains(&metadata.category) {
                registry.disable(&pass_id);
            }
        }
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registry_creation() {
        let registry = create_default_registry();
        assert!(registry.is_ok());

        let Ok(registry) = registry else {
            panic!("default registry creation should succeed")
        };
        let passes = registry.list_passes();

        // Should have all passes registered
        assert!(passes.len() > 20);
    }

    #[test]
    fn test_category_filtering() {
        use super::super::pass::PassCategory;

        let registry =
            create_registry_for_categories(&[PassCategory::ControlFlow, PassCategory::Naming]);

        assert!(registry.is_ok());
    }

    #[test]
    fn test_execute_default_passes() {
        let code = r#"
int test() {
    while (true) {
        if (x > 10) break;
        x++;
    }
    return x;
}
"#;

        let context = PassContext::new();
        let result = execute_default_passes(code, &context);

        assert!(result.is_ok());
        // Should have transformed while(true) to for loop or while(x <= 10)
    }

    #[test]
    fn test_execute_default_passes_with_stats() {
        let code = "int noop(){ return 0; }";
        let context = PassContext::new();
        let result = execute_default_passes_with_stats(code, &context);

        assert!(result.is_ok());
        let Ok((_output, stats)) = result else {
            panic!("execute_default_passes_with_stats should succeed")
        };
        assert!(stats.executed_passes > 0);
    }
}
