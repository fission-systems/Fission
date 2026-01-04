# Changelog

All notable changes to the Fission project.

---

## January 2026

### Pcode IR Optimizer Phase 2
- **Def-Use Tracking Infrastructure**: Implemented comprehensive def-use chain tracking with VarnodeId and DefUseInfo
- **NZMask Analysis**: Added non-zero mask computation for intelligent optimization
- **RuleShiftBitops**: Optimizes shifts that eliminate all non-zero bits (e.g., `(V & 0xf000) << 20 => #0`)
- **RuleAndMask**: AND operations optimized using NZMask (e.g., `V & 0xff => V` when appropriate)
- **Test Coverage**: 19 comprehensive tests covering all Phase 1 and Phase 2 rules

### Architecture Migration
- **Pool → FFI**: Migrated from subprocess pool to direct FFI integration via CXX bridge
- **Zero-Copy Decompilation**: Eliminated IPC overhead with native C++ bindings
- **Performance**: Significant reduction in decompilation latency

### Documentation Updates
- Updated README with FFI architecture details
- Added decompilation pipeline diagram
- Documented Pcode optimizer phases and coverage

---

## December 2025

### Decompiler Modularization
- **Modular Architecture**: Refactored monolithic decompiler into clean component structure
- **GCC/MinGW FID Support**: Added Function ID database support for GCC and MinGW compilers
- **FID Coverage**: 10 database files covering VS2012-2019 and legacy Windows SDK versions

### CLI Enhancements
- **One-Shot Mode**: Refactored into modular structure with dedicated command handlers
- **Command Separation**: Split analysis, decompilation, and function listing into focused modules
- **Error Handling**: Improved error messages and user feedback

---

## Earlier Updates

### Static Analysis
- **Multi-Format Support**: PE, ELF, and Mach-O binary parsing
- **iced-x86 Integration**: Pure Rust x86/x64 disassembly with syntax highlighting
- **Cross-Reference Analysis**: Automatic code and data xref detection
- **String Extraction**: ASCII and Unicode string detection with context

### Dynamic Analysis
- **Process Debugging**: Attach/detach with breakpoints and memory access
- **Time Travel Debugging**: Execution timeline with snapshot navigation
- **Live Memory Patching**: Real-time process memory modification
- **Cross-Platform**: Windows (Win32 API) and Linux (ptrace) support

### Smart Decompilation
- **Context-Aware Constants**: API parameter constant substitution (16 enum groups, 100+ mappings)
- **GDT Type Loading**: 5,700+ structures and 6,500+ typedefs from Ghidra
- **Auto-Inferred Structures**: Automatic structure layout detection
- **Reverse Type Propagation**: Type inference from callees to callers
- **VTable Analysis**: C++ virtual table recovery and indirect call resolution

### Extensibility
- **Plugin System**: Native Rust and Python plugin support via PyO3
- **Event Bus**: Subscribe to binary load, decompile, and debug events
- **Hook Priority**: Control plugin execution order
- **Python Scripting API**: Full access to binary info, functions, and sections

### User Interface
- **VS Code-Inspired GUI**: Tabbed interface with Catppuccin theming
- **Interactive REPL**: Command history and completion via reedline
- **Activity Bar**: Function explorer and search panels
- **Multi-View**: Assembly, decompiled C, hex, and debug views

---

## Statistics

- **Optimizer**: 32 optimization rules (~23% of Ghidra's 142 rules)
- **Code Base**: 1,765 lines in optimizer module alone
- **Test Coverage**: 19 passing tests with comprehensive validation
- **Platform Support**: Windows, Linux, and macOS
