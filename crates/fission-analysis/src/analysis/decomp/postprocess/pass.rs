//! Post-processing pass trait system
//!
//! Provides a trait-based abstraction for decompiler post-processing passes
//! with support for:
//! - Dynamic enable/disable
//! - Dependency resolution
//! - Execution ordering
//! - Pass metadata and configuration

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Metadata about a post-processing pass
#[derive(Debug, Clone)]
pub struct PassMetadata {
    /// Unique identifier for the pass
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// Brief description of what the pass does
    pub description: &'static str,
    /// Category for grouping (e.g., "arithmetic", "control-flow", "naming")
    pub category: PassCategory,
}

/// Category classification for passes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassCategory {
    /// Expression simplification and arithmetic optimization
    Arithmetic,
    /// Control flow restructuring (loops, conditionals, switches)
    ControlFlow,
    /// Variable and function naming
    Naming,
    /// Dead code and redundancy elimination
    Cleanup,
    /// Language-specific boilerplate removal
    LanguageSpecific,
    /// Type-based transformations
    TypeBased,
}

/// Result type for pass execution
pub type PassResult<'a> = Result<Cow<'a, str>, PassError>;

/// Aggregated statistics from a pass pipeline execution.
#[derive(Debug, Clone, Default)]
pub struct PassExecutionStats {
    /// Number of passes that actually ran (not skipped)
    pub executed_passes: usize,
    /// Number of passes skipped because they were disabled
    pub skipped_disabled: usize,
    /// Number of passes skipped by `should_run` condition
    pub skipped_should_run: usize,
    /// Number of executed passes that returned `Cow::Borrowed`
    pub borrowed_outputs: usize,
    /// Number of executed passes that returned `Cow::Owned`
    pub owned_outputs: usize,
}

impl PassExecutionStats {
    /// Ratio of borrowed outputs among executed passes.
    pub fn borrowed_ratio(&self) -> f64 {
        if self.executed_passes == 0 {
            0.0
        } else {
            self.borrowed_outputs as f64 / self.executed_passes as f64
        }
    }
}

/// Errors that can occur during pass execution
#[derive(Debug, thiserror::Error)]
pub enum PassError {
    #[error("Pass {0} failed: {1}")]
    ExecutionFailed(String, String),

    #[error("Dependency {0} not found for pass {1}")]
    MissingDependency(String, String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Pass {0} is disabled")]
    PassDisabled(String),
}

/// Trait for decompiler post-processing passes
///
/// Each pass should implement this trait to participate in the
/// post-processing pipeline with automatic dependency resolution
/// and configurable execution.
pub trait PostProcessPass: Send + Sync {
    /// Get metadata about this pass
    fn metadata(&self) -> PassMetadata;

    /// Execute the pass on the given code
    ///
    /// # Arguments
    /// * `code` - The input C code to transform
    /// * `context` - Shared context for accessing type info, DWARF data, etc.
    ///
    /// # Returns
    /// Transformed code or an error
    fn run<'a>(&self, code: &'a str, context: &PassContext) -> PassResult<'a>;

    /// Get the list of pass IDs that must run before this pass
    ///
    /// Returns an empty slice if the pass has no dependencies.
    fn dependencies(&self) -> &[&'static str] {
        &[]
    }

    /// Check if this pass should run based on context
    ///
    /// Default implementation always returns true.
    /// Override to implement conditional execution logic.
    fn should_run(&self, _context: &PassContext) -> bool {
        true
    }
}

/// Shared context for pass execution
///
/// Contains information that passes may need to access during execution,
/// such as type information, DWARF data, and configuration options.
#[derive(Debug, Default)]
pub struct PassContext {
    /// Inferred type information for field name resolution
    pub inferred_types: Vec<fission_loader::loader::types::InferredTypeInfo>,

    /// DWARF function info for variable/parameter name substitution
    pub dwarf_info: Option<fission_loader::loader::types::DwarfFunctionInfo>,

    /// Custom key-value data for pass-specific configuration
    pub metadata: HashMap<String, String>,
}

impl PassContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set inferred types
    pub fn with_inferred_types(
        mut self,
        types: Vec<fission_loader::loader::types::InferredTypeInfo>,
    ) -> Self {
        self.inferred_types = types;
        self
    }

    /// Set DWARF function info
    pub fn with_dwarf_info(
        mut self,
        info: Option<fission_loader::loader::types::DwarfFunctionInfo>,
    ) -> Self {
        self.dwarf_info = info;
        self
    }

    /// Set a metadata value
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get a metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

/// Registry for managing post-processing passes
///
/// Handles pass registration, dependency resolution, and execution ordering.
pub struct PassRegistry {
    /// Registered passes
    passes: HashMap<String, Box<dyn PostProcessPass>>,

    /// Pass execution order (computed from dependencies)
    execution_order: Vec<String>,

    /// Enabled/disabled state for each pass
    enabled: HashMap<String, bool>,
}

impl PassRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            passes: HashMap::new(),
            execution_order: Vec::new(),
            enabled: HashMap::new(),
        }
    }

    /// Register a pass
    ///
    /// Returns an error if a pass with the same ID is already registered.
    pub fn register(&mut self, pass: Box<dyn PostProcessPass>) -> Result<(), String> {
        let id = pass.metadata().id;

        if self.passes.contains_key(id) {
            return Err(format!("Pass '{}' is already registered", id));
        }

        self.passes.insert(id.to_string(), pass);
        self.enabled.insert(id.to_string(), true); // Enable by default

        // Recompute execution order
        self.compute_execution_order()?;

        Ok(())
    }

    /// Enable a pass by ID
    pub fn enable(&mut self, pass_id: &str) {
        self.enabled.insert(pass_id.to_string(), true);
    }

    /// Disable a pass by ID
    pub fn disable(&mut self, pass_id: &str) {
        self.enabled.insert(pass_id.to_string(), false);
    }

    /// Check if a pass is enabled
    pub fn is_enabled(&self, pass_id: &str) -> bool {
        self.enabled.get(pass_id).copied().unwrap_or(false)
    }

    /// Execute all enabled passes in dependency order
    pub fn execute_all<'a>(&self, code: &'a str, context: &PassContext) -> PassResult<'a> {
        let (output, _) = self.execute_all_with_stats(code, context)?;
        Ok(output)
    }

    /// Execute all enabled passes in dependency order with execution statistics.
    pub fn execute_all_with_stats<'a>(
        &self,
        code: &'a str,
        context: &PassContext,
    ) -> Result<(Cow<'a, str>, PassExecutionStats), PassError> {
        let mut current: Cow<'a, str> = Cow::Borrowed(code);
        let mut stats = PassExecutionStats::default();

        for pass_id in &self.execution_order {
            if !self.is_enabled(pass_id) {
                stats.skipped_disabled += 1;
                continue;
            }

            let pass = self.passes.get(pass_id).ok_or_else(|| {
                PassError::ExecutionFailed(
                    pass_id.clone(),
                    "Pass not found in registry".to_string(),
                )
            })?;

            if !pass.should_run(context) {
                stats.skipped_should_run += 1;
                continue;
            }

            stats.executed_passes += 1;
            let next = pass.run(current.as_ref(), context)?;
            match next {
                Cow::Borrowed(_) => {
                    stats.borrowed_outputs += 1;
                }
                Cow::Owned(s) => {
                    stats.owned_outputs += 1;
                    current = Cow::Owned(s);
                }
            }
        }

        Ok((current, stats))
    }

    /// Get a list of all registered pass IDs
    pub fn list_passes(&self) -> Vec<String> {
        self.passes.keys().cloned().collect()
    }

    /// Get metadata for a specific pass
    pub fn get_metadata(&self, pass_id: &str) -> Option<PassMetadata> {
        self.passes.get(pass_id).map(|p| p.metadata())
    }

    /// Compute execution order using topological sort
    fn compute_execution_order(&mut self) -> Result<(), String> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        for pass_id in self.passes.keys() {
            if !visited.contains(pass_id.as_str()) {
                self.visit_pass(pass_id, &mut visited, &mut visiting, &mut order)?;
            }
        }

        self.execution_order = order;
        Ok(())
    }

    /// Depth-first search for topological sort
    fn visit_pass(
        &self,
        pass_id: &str,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), String> {
        if visiting.contains(pass_id) {
            return Err(format!(
                "Circular dependency detected involving '{}'",
                pass_id
            ));
        }

        if visited.contains(pass_id) {
            return Ok(());
        }

        visiting.insert(pass_id.to_string());

        let pass = self
            .passes
            .get(pass_id)
            .ok_or_else(|| format!("Pass '{}' not found", pass_id))?;

        for dep_id in pass.dependencies() {
            if !self.passes.contains_key(*dep_id) {
                return Err(format!(
                    "Dependency '{}' not found for pass '{}'",
                    dep_id, pass_id
                ));
            }

            self.visit_pass(dep_id, visited, visiting, order)?;
        }

        visiting.remove(pass_id);
        visited.insert(pass_id.to_string());
        order.push(pass_id.to_string());

        Ok(())
    }
}

impl Default for PassRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPass {
        id: &'static str,
        deps: Vec<&'static str>,
    }

    struct BorrowPass {
        id: &'static str,
    }

    impl PostProcessPass for TestPass {
        fn metadata(&self) -> PassMetadata {
            PassMetadata {
                id: self.id,
                name: self.id,
                description: "Test pass",
                category: PassCategory::Cleanup,
            }
        }

        fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
            Ok(Cow::Owned(format!("{}:{}", self.id, code)))
        }

        fn dependencies(&self) -> &[&'static str] {
            &self.deps
        }
    }

    impl PostProcessPass for BorrowPass {
        fn metadata(&self) -> PassMetadata {
            PassMetadata {
                id: self.id,
                name: self.id,
                description: "Borrow pass",
                category: PassCategory::Cleanup,
            }
        }

        fn run<'a>(&self, code: &'a str, _context: &PassContext) -> PassResult<'a> {
            Ok(Cow::Borrowed(code))
        }
    }

    #[test]
    fn test_pass_registration() {
        let mut registry = PassRegistry::new();
        let pass = Box::new(TestPass {
            id: "test",
            deps: vec![],
        });

        assert!(registry.register(pass).is_ok());
        assert!(registry.is_enabled("test"));
    }

    #[test]
    fn test_dependency_resolution() {
        let mut registry = PassRegistry::new();

        assert!(
            registry
                .register(Box::new(TestPass {
                    id: "a",
                    deps: vec![]
                }))
                .is_ok()
        );
        assert!(
            registry
                .register(Box::new(TestPass {
                    id: "b",
                    deps: vec!["a"]
                }))
                .is_ok()
        );
        assert!(
            registry
                .register(Box::new(TestPass {
                    id: "c",
                    deps: vec!["b"]
                }))
                .is_ok()
        );

        assert_eq!(registry.execution_order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut registry = PassRegistry::new();

        // This would create a cycle, but our simplified test doesn't allow it
        // In real implementation, we'd need to detect this
        assert!(
            registry
                .register(Box::new(TestPass {
                    id: "a",
                    deps: vec![]
                }))
                .is_ok()
        );

        // Can't easily test circular deps with current structure without
        // more complex setup, so this is a placeholder
    }

    #[test]
    fn test_execute_all_with_stats_counts_borrowed_and_owned() {
        let mut registry = PassRegistry::new();
        assert!(
            registry
                .register(Box::new(BorrowPass { id: "borrow" }))
                .is_ok()
        );
        assert!(
            registry
                .register(Box::new(TestPass {
                    id: "owned",
                    deps: vec!["borrow"],
                }))
                .is_ok()
        );

        let context = PassContext::new();
        let result = registry.execute_all_with_stats("x", &context);
        assert!(result.is_ok());

        let Ok((output, stats)) = result else {
            panic!("execute_all_with_stats should succeed")
        };
        assert_eq!(output.as_ref(), "owned:x");
        assert_eq!(stats.executed_passes, 2);
        assert_eq!(stats.borrowed_outputs, 1);
        assert_eq!(stats.owned_outputs, 1);
        assert_eq!(stats.skipped_disabled, 0);
        assert_eq!(stats.skipped_should_run, 0);
        assert!((stats.borrowed_ratio() - 0.5).abs() < f64::EPSILON);
    }
}
