# Fission Architecture Documentation

## Module Structure

### Core Modules

#### `src/core/` - Core Infrastructure
- `errors.rs` - Unified error handling with `FissionError` enum
- `config.rs` - Configuration management
- `logging.rs` - Logging infrastructure
- `prelude.rs` - Common imports

#### `src/analysis/` - Binary Analysis
- `loader/` - Binary parsing (PE, ELF, Mach-O)
- `disasm/` - Disassembly engine (iced-x86)
- `decomp/` - Decompilation (FFI to Ghidra)
- `signatures/` - Windows API signatures
- `xrefs/` - Cross-reference analysis

#### `src/ui/` - User Interfaces
- `gui/` - egui-based GUI
- `cli/` - Command-line REPL interface

### Analysis Pipeline

#### Static Analysis Layer
**`src/analysis/`** - Binary file analysis
- `loader/` - Parses PE/ELF/Mach-O from **disk files**
- `disasm/` - Static disassembly
- `signatures/` - API signatures and patterns

#### Dynamic Analysis Layer  
**`src/unpacker/`** - Runtime memory analysis (formerly `debug_engine`)
- **Purpose**: IAT reconstruction, process dumping, PE fixing
- **Input**: Running process memory
- **Use Cases**: Unpacking, malware analysis, forensics
- **NOT a debugger** - Focuses on memory extraction/reconstruction

#### Interactive Debugging Layer
**`src/debug/`** - Live process debugging
- **Purpose**: Interactive debugging (breakpoints, stepping, memory manipulation)
- **Input*unpacker/` - Runtime Memory Analysis (ACTIVE)
**Purpose**: Import reconstruction, process dumping, PE fixing
**Status**: Active, Windows-only specialized tool
**Platform**: Windows only
**Key Features**:
- IAT (Import Address Table) reconstruction from memory
- Process dumping with PE fixing
- Unpacking packed/obfuscated executables

**Use Cases**:
- Malware analysis
- Unpacking commercial packers
- Memory forensics
- Executable reconstruction

**Not a Debugger**: Despite using debug APIs for memory access, this module
focuses on **extraction and reconstruction**, not interactive debugging.

### ✅ Module Clarification (Updated)

**Previously Confusing**: `debug_engine` sounded like a debugger but was actually
an unpacking/dumping tool.

**Now Clear**: 
- `debug/` = Interactive debugging
- `unpacker/` = Memory analysis & reconstruction

No more type duplication issues - these are completely different tools with
different purposes. Debugging in `ttd/`

#### `src/debug_engine/` - Legacy TitanEngine (DEPRECATED)
**Purpose**: Windows-only debugging engine
**Status**: Legacy code, to be phased out
**Platform**: Windows only
**Key Types** (DUPLICATE):
- `ProcessInfo` - Windows-specific process info with HANDLE
- `Breakpoint` - Windows-specific breakpoint
- `TitanEngine` - Main engine struct

**Issues**:
- Duplicates types from `src/debug/`
- Windows-specific, not cross-platform
- Tightly coupled to Windows APIs
- No clear migration path

### ⚠️ CRITICAL: Type Duplication

**Problem**: Same type names in different modules

| Type | `debug/types.rs` | `debug_engine/types.rs` | Status |
|------|------------------|-------------------------|--------|
| `ProcessInfo` | ✅ Cross-platform | ❌ Windows-only | Duplicate |
| `Breakpoint` | ✅ Generic | ❌ Windows-specific | Duplicate |

**Impact**:
- Confusing imports
- Maintenance burden
- Risk of using wrong type
- Prevents clean migration

## Planned Refactoring

### Phase 1: Immediate (✅ COMPLETED)
1. ✅ Remove `unwrap()` calls throughout codebase
2. ✅ Add FFI safety validation to `DecompilerNative`
3. ✅ Improve error handling consistency
4. ✅ Document architecture issues

### Phase 2: Type Consolidation (✅ RESOLVED)

**Resolution**: Modules clarified - no actual duplication.

- `debug/` types are for **interactive debugging**
- `unpacker/` types are for **memory analysis/dumping**
- Different purposes, different contexts
- No migration needed - intentional design

### Phase 3: Module Cleanup
1. **Remove Dead Code**:
   - `src/parser/` - Nearly empty, unused
   - Unused FFI bindings
   - Old commented-out code

2. **Reduce Lint Allowances**:
   - Fix actual issues instead of silencing
   - Remove `#![allow(dead_code)]`
   - Remove `#![allow(unused_imports)]`

3. **Improve Synchronization**:
   - Document Arc<Mutex<>> usage
   - Consider using channels instead of shared state
   - Remove global static TOKIO_RUNTIME

### Phase 4: Testing
1. Add unit tests for core modules
2. Add integration tests for debugger
3. Add FFI safety tests
4. Add error handling tests

## Design Principles

### Error Handling
- ✅ Use `Result<T>` for all fallible operations
- ✅ Never use `unwrap()` in production code
- ✅ Use `expect()` only for truly impossible cases
- ✅ Propagate errors with `?` operator
- ✅ Provide meaningful error messages

### FFI Safety
- ✅ Validate all inputs before FFI calls
- ✅ Never pass null pointers from safe code
- ✅ Check return values from C/C++
- ✅ Use RAII for resource management
- ✅ Track object validity to prevent use-after-free

### Module Organization
- Keep platform-specific code isolated
- Use traits for cross-platform abstractions
- Clear separation of concerns
- Avoid circular dependencies

### Concurrency
- Prefer message passing over shared state
- Document thread safety assumptions
- Use appropriate sync primitives (Mutex vs RwLock vs channels)
- Avoid global mutable state

## Migration Checklist

### ✅ Module Rename (COMPLETED)

- [x] Rename `debug_engine/` to `unpacker/`
- [x] Update module declaration in `src/lib.rs`
- [x] Update imports in `titan_ops.rs`
- [x] Update documentation with clear purpose
- [x] Clarify that it's NOT a debugger
- [x] Update ARCHITECTURE.md

### parser Module Cleanup

- [ ] Check if `parser/` is actually used
- [ ] Move any used code to `analysis/loader`
- [ ] Delete empty `parser/` module
- [ ] Update `src/lib.rs` module exports

## Notes

- `debug/` is the future - modern, cross-platform, well-architected
- `debug_engine/` is legacy - Windows-only, needs migration
- Priority: Complete type consolidation before adding new features
- Goal: Single source of truth for all types
