# Changelog

All notable changes to the Fission project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Swift Support

- **Swift Symbol Demangling**: Integrated Swift symbol demangling using external `swift demangle` command
  - Automatically detects Swift symbols (`_$s...`, `__T...`, `_T...` prefixes)
  - Demangled names appear in function lists and decompiled output
  
- **Swift Metadata Parsing**: Parse Swift 5 runtime metadata sections for type recovery
  - `__swift5_fieldmd`: Field descriptor parsing for struct/class fields
  - `__swift5_reflstr`: Field name extraction
  - `__swift5_typeref`: Type reference parsing
  - Extracts field names, types (SS=String, Si=Int), and offsets

#### Type Recovery Infrastructure

- **`InferredTypeInfo` System**: New data structures for storing recovered type information
  - `InferredTypeInfo`: Struct/class definition with name, kind, size, and fields
  - `InferredFieldInfo`: Individual field with name, type, offset, and size
  - Serializable with rkyv for caching
  
- **FFI Type Registration API**: Bridge between Rust metadata and C++ Ghidra decompiler
  - `decomp_register_struct_type()`: Register struct types with TypeFactory
  - `decomp_apply_struct_to_param()`: Apply types to function parameters
  - `DecompFieldInfo` FFI struct for passing field data
  
- **Ghidra TypeFactory Integration**: C++ implementation for type registration
  - Creates `TypeStruct` with named fields at specified offsets
  - Supports common Swift type mappings (Si→int, SS→char*, Sf→float)
  - Stores registered types in `DecompContext` for reuse

#### Enhanced Demangling

- Unified `demangle()` function now handles:
  - Rust symbols (`_R`, `_ZN...rust...`)
  - C++ Itanium/GNU symbols (`_Z...`)
  - MSVC symbols (`?...`)
  - **NEW**: Swift symbols (`_$s...`, `__T...`)

#### Go Analysis Improvements

- Fixed function name extraction from `.gopclntab` section
- Improved handling of stripped Go binaries

### Changed

- **`LoadedBinary`**: Added `inferred_types` field to store recovered type metadata
- **`PostProcessor`**: Enhanced to accept inferred types for field name resolution
- **`CachingDecompiler`**: Now stores and uses inferred types during decompilation
- **CLI Decompile Command**: Registers inferred types before decompilation

### Fixed

- Fixed duplicate `AnalysisArtifacts` variable definition in `DecompilationCore.cpp`
- Fixed borrow checker issues in Swift metadata analysis (separate scopes for analyzers)

### Technical Details

#### Architecture Overview

```
Swift Binary
    ↓
AppleAnalyzer::analyze_swift_types()
    ↓
InferredTypeInfo (Rust)
    ↓
DecompilerNative::register_inferred_types()
    ↓
decomp_register_struct_type() (FFI)
    ↓
Ghidra TypeFactory::getTypeStruct()
    ↓
TypeStruct with TypeFields
    ↓
ptr->fieldName in output
```

#### Files Modified

- `crates/fission-loader/src/loader/demangle.rs` - Swift demangling
- `crates/fission-loader/src/loader/macho/apple.rs` - Swift metadata parsing
- `crates/fission-loader/src/loader/types.rs` - InferredTypeInfo structs
- `crates/fission-loader/src/loader/mod.rs` - Integration
- `crates/fission-ffi/src/decomp.rs` - FFI bindings
- `crates/fission-analysis/src/analysis/decomp/postprocess.rs` - Field replacement
- `crates/fission-analysis/src/analysis/decomp/mod.rs` - CachingDecompiler update
- `crates/fission-cli/src/ui/cli/handlers/commands/decompile.rs` - CLI integration
- `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h` - FFI header
- `ghidra_decompiler/include/fission/ffi/DecompContext.h` - Context fields
- `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp` - Type registration implementation

## [0.1.0] - Initial Release

- Core decompilation engine with Ghidra integration
- PE, ELF, Mach-O binary format support
- GUI and CLI interfaces
- Function ID (FID) matching
- Basic symbol management
