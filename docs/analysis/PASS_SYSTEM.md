# Post-Processing Pass System

**Phase 5.2 Implementation** - Trait-based decompiler post-processing architecture

## Overview

The pass system provides a flexible, trait-based framework for managing decompiler post-processing operations. It enables:

- **Dynamic configuration**: Enable/disable passes at runtime
- **Dependency management**: Automatic execution ordering based on dependencies
- **Extensibility**: Easy addition of custom passes without modifying core code
- **Testability**: Individual passes can be tested in isolation
- **Plugin support**: Foundation for future plugin architecture

## Core Architecture

### Pass Trait

All post-processing operations implement the `PostProcessPass` trait:

```rust
pub trait PostProcessPass: Send + Sync {
    /// Metadata about this pass (name, category, description)
    fn metadata(&self) -> PassMetadata;
    
    /// Execute the pass on the given code
    fn run(&self, code: &str, context: &PassContext) -> PassResult;
    
    /// List of pass IDs this pass depends on
    fn dependencies(&self) -> Vec<String> { vec![] }
    
    /// Check if this pass should run based on context
    fn should_run(&self, _context: &PassContext) -> bool { true }
}
```

### Pass Categories

Passes are organized into 6 categories:

- **Arithmetic**: Arithmetic idiom recognition and optimization
- **ControlFlow**: Loop and conditional structure recognition
- **Naming**: Variable and function naming improvements
- **Cleanup**: Dead code elimination and simplification
- **LanguageSpecific**: Language-specific boilerplate removal
- **TypeBased**: Type-driven transformations

### Pass Registry

The `PassRegistry` manages all passes and handles:

- **Registration**: Add passes to the registry
- **Dependency resolution**: Topological sort for execution order
- **Enable/disable**: Control which passes run
- **Execution**: Run all enabled passes in correct order

```rust
use fission_analysis::analysis::decomp::postprocess::registry;

// Create a registry with all default passes
let mut registry = registry::create_default_registry()?;

// Disable specific passes
registry.disable("remove_rust_boilerplate");

// Execute all enabled passes
let result = registry.execute_all(code, &context)?;
```

## Available Passes

### Language-Specific (3 passes)

| Pass ID | Description | Category |
|---------|-------------|----------|
| `remove_rust_boilerplate` | Remove Rust panic handlers and safety checks | LanguageSpecific |
| `remove_go_boilerplate` | Remove Go runtime boilerplate | LanguageSpecific |
| `swift_demangle` | Demangle Swift symbols | LanguageSpecific |

### Type-Based (3 passes)

| Pass ID | Description | Category | Dependencies |
|---------|-------------|----------|--------------|
| `field_offset_replacement` | Replace `*(ptr + offset)` with field names | TypeBased | - |
| `insert_missing_casts` | Insert type casts for assignments | TypeBased | - |
| `apply_dwarf_names` | Apply DWARF variable/parameter names | TypeBased | - |

### Arithmetic (2 passes)

| Pass ID | Description | Category |
|---------|-------------|----------|
| `arithmetic_idioms` | Recognize arithmetic idioms (e.g., `x & 1` → `is_odd`) | Arithmetic |
| `mul_pow2_to_shift` | Convert power-of-2 multiplication to shifts | Arithmetic |

### Cleanup (4 passes)

| Pass ID | Description | Category | Dependencies |
|---------|-------------|----------|--------------|
| `deref_to_array_index` | Convert `*(a + N)` to `a[N]` | Cleanup | - |
| `bitop_to_logicop` | Convert bitwise ops to logical ops in conditions | Cleanup | - |
| `remove_constant_conditions` | Remove always-true/false branches | Cleanup | - |
| `remove_dead_assignments` | Remove unused assignments (2 iterations) | Cleanup | - |

### Control Flow (8 passes)

| Pass ID | Description | Category | Dependencies |
|---------|-------------|----------|--------------|
| `simplify_if_structure` | Remove empty else, apply early return | ControlFlow | `remove_constant_conditions` |
| `while_true_to_cond` | `while(true) { if(c) break; }` → `while(!c)` | ControlFlow | `simplify_if_structure` |
| `while_true_to_for` | `while(true)` with loop counter → `for` | ControlFlow | `while_true_to_cond` |
| `while_cond_to_for` | `while(cond)` with init/incr → `for` | ControlFlow | `while_true_to_for` |
| `do_while_to_for` | `do-while` with counter → `for` | ControlFlow | `while_cond_to_for` |
| `while_true_to_for_ever` | `while(true)` → `for(;;)` | ControlFlow | `do_while_to_for` |
| `switch_reconstruction` | BST pattern → `switch` statement | ControlFlow | - |
| `switch_from_if_else_assign` | `if-else` assignment chain → `switch` | ControlFlow | `switch_reconstruction` |

### Naming (3 passes)

| Pass ID | Description | Category | Dependencies |
|---------|-------------|----------|--------------|
| `rename_induction_vars` | Rename loop counters to `i`, `j`, `k` | Naming | All control flow passes |
| `rename_semantic_vars` | Rename to `argc`, `argv`, `result`, etc. | Naming | - |
| `loop_idioms` | Recognize idioms (strlen, memset, etc.) | Naming | `rename_induction_vars` |

## Usage Examples

### Basic Usage (Recommended)

```rust
use fission_analysis::analysis::decomp::postprocess::PostProcessor;

let processor = PostProcessor::new();
let result = processor.process_with_registry(code)?;
```

### Custom Configuration

```rust
use fission_analysis::analysis::decomp::postprocess::{
    PostProcessor,
    RustPostProcessOptions,
};

let options = RustPostProcessOptions {
    clean_rust: true,
    while_to_for: true,
    switch_reconstruction: false, // Disable switch reconstruction
    ..Default::default()
};

let processor = PostProcessor::new().with_options(options);
let result = processor.process_with_registry(code)?;
```

### Direct Registry Usage

```rust
use fission_analysis::analysis::decomp::postprocess::{
    pass::PassContext,
    registry,
};

// Create a registry with all passes
let mut pass_registry = registry::create_default_registry()?;

// Disable specific passes
pass_registry.disable("remove_rust_boilerplate");
pass_registry.disable("switch_reconstruction");

// Create context
let context = PassContext::new();

// Execute passes
let result = pass_registry.execute_all(code, &context)?;
```

### Category-Specific Processing

```rust
use fission_analysis::analysis::decomp::postprocess::{
    pass::{PassCategory, PassContext},
    registry,
};

// Only run control flow and naming passes
let pass_registry = registry::create_registry_for_categories(&[
    PassCategory::ControlFlow,
    PassCategory::Naming,
])?;

let context = PassContext::new();
let result = pass_registry.execute_all(code, &context)?;
```

## Creating Custom Passes

### Step 1: Implement the Trait

```rust
use fission_analysis::analysis::decomp::postprocess::pass::{
    PostProcessPass, PassMetadata, PassCategory, PassContext, PassResult,
};

pub struct MyCustomPass;

impl PostProcessPass for MyCustomPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "my_custom_pass".to_string(),
            name: "My Custom Pass".to_string(),
            description: "Performs custom transformations".to_string(),
            category: PassCategory::Cleanup,
        }
    }
    
    fn run(&self, code: &str, _context: &PassContext) -> PassResult {
        // Transform code here
        Ok(code.to_string())
    }
    
    fn dependencies(&self) -> Vec<String> {
        vec!["remove_constant_conditions".to_string()]
    }
}
```

### Step 2: Register the Pass

```rust
let mut registry = registry::create_default_registry()?;
registry.register(Box::new(MyCustomPass))?;
```

## Dependency Resolution

The registry automatically orders passes based on dependencies using topological sort:

```rust
// If PassA depends on PassB, PassB runs first
PassB → PassA

// Circular dependencies are detected and rejected
PassA → PassB → PassC → PassA  // ERROR: Circular dependency
```

## Error Handling

The pass system uses the `PassError` type:

```rust
pub enum PassError {
    #[error("Pass not found: {0}")]
    NotFound(String),
    
    #[error("Circular dependency detected: {0:?}")]
    CircularDependency(Vec<String>),
    
    #[error("Pass already registered: {0}")]
    AlreadyRegistered(String),
    
    #[error("Pass execution failed: {pass_id}: {message}")]
    ExecutionFailed { pass_id: String, message: String },
}
```

## Performance Considerations

- **Dependency Resolution**: O(V + E) using topological sort
- **Pass Execution**: Linear in number of enabled passes
- **Context Sharing**: Passes share context to avoid redundant computation
- **Early Exit**: `should_run()` allows passes to skip execution

## Testing

### Unit Testing Individual Passes

```rust
#[test]
fn test_my_pass() {
    let pass = MyCustomPass;
    let context = PassContext::new();
    let code = "int x = 1;";
    let result = pass.run(code, &context).unwrap();
    assert_eq!(result, "/* optimized */ int x = 1;");
}
```

### Integration Testing

```rust
#[test]
fn test_full_pipeline() {
    let code = "while (true) { if (x > 10) break; x++; }";
    let context = PassContext::new();
    let result = registry::execute_default_passes(code, &context).unwrap();
    // Should transform to for loop
    assert!(result.contains("for"));
}
```

## Future Extensions

The pass system is designed to support:

- **Plugin Architecture**: Load passes from dynamic libraries
- **Configuration Files**: TOML/JSON-based pass configuration
- **Pass Metrics**: Collect statistics on pass execution time
- **Caching**: Cache pass results for unchanged code
- **Parallel Execution**: Run independent passes in parallel

## Migration Guide

### From Legacy `process()` Method

Old code:
```rust
let processor = PostProcessor::new();
let result = processor.process(code);
```

New code (recommended):
```rust
let processor = PostProcessor::new();
let result = processor.process_with_registry(code)?; // Now returns Result
```

The legacy `process()` method is still available for backward compatibility but will be deprecated in a future release.

## Related Documentation

- [Architecture Overview](../architecture/ARCHITECTURE.md)
- [Post-Processing Analysis](IMPROVEMENT_LOG.md)
- [Control Flow Analysis](../cfg_analysis.md)
