# Changelog

All notable changes to the Fission project (November 2025 - January 2026).

---

## Recent Updates

### Decompiler Output & Tooling Improvements (2026-01-06)
- **String literal inlining**: Decompiler now replaces string addresses with actual literals for readability
- **Global symbol normalization**: `pg_`/`uRam`/`xRam`/`pxRam` renamed to `g_`/`gp_` for cleaner output
- **GDT prototype enforcement (FFI path)**: IAT prototypes are now applied during FFI decompilation
- **One-shot CLI polish**: `--strings [min_len]` support, no trailing help after `--decomp`, quieter native logs by default
- **Comparison tooling stability (macOS)**: `compare_decompilers_v2.sh` switched to Python timeout and preserves `DYLD_LIBRARY_PATH`
- **On-demand symbol provider**: Added Scope-backed symbol query pipeline (functions/data) for richer global name resolution
- **Symbol range estimation**: Data/function sizes are inferred from section boundaries to improve address-range lookups
- **Readonly propagation**: Section permissions are registered into the property map for better constant folding
- **Crash fix**: CLI decompiler now initializes the Database before querying global scope

### COFF Symbol Table Implementation (2026-01-05)

**Critical Achievement:**
- **PE Symbol Table Parser** - Implemented complete COFF symbol table parsing for MinGW binaries
  - Added `CoffSymbol` structure with binrw parsing support
  - Parse symbol name (short 8-byte or long string table reference)
  - Handle auxiliary symbols correctly (skip in iteration)
  - Filter by storage class (C_EXT, C_STAT) and symbol type (DT_FCN)
  - Section-relative address calculation
  - Location: `src/analysis/loader/pe/mod.rs`, `src/analysis/loader/pe/schema.rs`

- **100% MinGW Function Recognition** - Achieved parity with Ghidra for MinGW-compiled binaries
  - **Before**: 41% recognition (11/27 functions, import table only)
  - **After**: 100% recognition (124/124 functions, imports + symbols)
  - Function names now correctly resolved:
    - `__tmainCRTStartup` (was `FUN_0x140001010`)
    - `__main` (was `FUN_0x140001890`)
    - `main`, `add`, `multiply`, `print_message` (all user functions)
  - All MinGW CRT functions identified with real names

**Root Cause Analysis:**
- **Ghidra's Strategy**: Uses symbol table as primary source (FID only for stripped binaries)
- **MinGW vs MSVC Difference**:
  - MinGW: Ships with COFF symbol table by default → FID database unnecessary
  - MSVC (Release): Strips symbols → Requires FID database for function identification
- **Symbol Priority**: Symbol Table > Export/Import Table > FID Database > PDATA

**Implementation Details:**
- **Auxiliary Symbol Handling**: Correctly skip auxiliary records (each symbol can have 0-N aux records)
- **String Table**: Parse long names from string table (starts at symbol_table_offset + symbol_count * 18)
- **Storage Class Filtering**: Process C_EXT (external) and C_STAT (static) symbols
- **Type Checking**: Verify symbol type has DT_FCN (function) in high nibble
- **Address Calculation**: Combine section base address with symbol value offset

**Testing Results:**
- MinGW x64 test binary: 84 COFF functions discovered
- All user-defined functions correctly named
- All MinGW runtime functions identified
- Zero false positives

**Known Limitations:**
- COFF symbols don't provide function sizes (size field always 0)
- Relies on PDATA or heuristic analysis for function boundaries
- Only applicable to non-stripped PE binaries

### Decompiler Comparison & Mach-O Improvements (2026-01-05)

**Critical Fixes:**
- **ARM64 Architecture Recognition** - Fixed Mach-O parser misidentifying ARM64 binaries as x86_64
  - CPU type detection now properly handles `0x100000C` (ARM64) and `0x1000007` (x86_64)
  - Architecture display updated to show "ARM64 (64-bit)" or "x86_64 (64-bit)" correctly
  - Warning messages for unknown CPU types
  - Location: `src/analysis/loader/macho/mod.rs`, `src/cli/oneshot/binary_info.rs`

- **External Function Symbol Resolution** - Implemented Mach-O dynamic symbol parsing
  - Added `LC_DYSYMTAB` load command parsing with `DysymtabCommand` structure
  - Parse indirect symbol table to resolve `__stubs` section entries
  - Parse GOT (`__got`) section for indirect function pointers
  - External functions now display as `_printf()`, `_malloc()`, `_free()` instead of `gp_0xXXXXXXXX`
  - IAT symbols increased from 0 to 8+ per binary (stubs + GOT entries)
  - Location: `src/analysis/loader/macho/schema.rs`, `src/analysis/loader/macho/mod.rs`

**Testing & Validation:**
- **PyGhidra Integration** - Created automated comparison framework
  - `scripts/pyghidra_decompile.py`: Python wrapper for Ghidra decompilation
  - `scripts/compare_decompilers.sh`: Side-by-side comparison script with assembly listing
  - Supports PE, ELF, and Mach-O formats
  - Displays Ghidra assembly + decompiled code, Fission disassembly + decompiled code
  - PyGhidra 2.2.0 compatibility with Ghidra 11.4.2

- **Comparison Test Suite** - New test binaries for systematic evaluation
  - `test/comparison_test.c`: Multi-feature C test program
    - Simple arithmetic (add, multiply)
    - External function calls (printf, malloc, free)
    - Struct operations (init, print, create, destroy)
    - Control flow (if-else chains)
    - Loops (for iteration)
  - Built with MinGW x86-64 for Windows PE format
  - Documentation: `test/README_COMPARISON.md`
  - Detailed analysis: `docs/DECOMPILER_COMPARISON.md`

**Known Issues Identified:**
- ⚠️ COFF symbol table not parsed (PE function names show as `FUN_0xXXXXXXXX`)
- ⚠️ Calling convention not implemented (parameters show as `unaff_RCX`, `unaff_RDX`)
- ⚠️ Complex functions show "Unreachable block" false positives
- ⚠️ PIC/GOT indirect calls treated as indirect jumps
- ⚠️ Type inference needs improvement (struct pointers, complex types)

**Performance:**
- Simple functions (add, multiply): Near-identical to Ghidra
- Complex functions (malloc/free chains): Logic correct but names/types need work
- External function recognition: 100% success rate on tested binaries

### Pcode Graph Visualization System (2026-01-05)
- **CLI Graph Command**: Added `--graph` option to generate Pcode control flow graphs
  - Generates DOT format graphs with automatic PNG rendering (via Graphviz)
  - Supports custom output file paths with `-o` option
  - Example: `fission_cli binary.exe --graph 0x401000 -o my_graph.dot`
- **Assembly Integration**: Each Pcode operation now displays its original assembly instruction
  - Implemented `SimpleAssemblyEmit` class in C++ backend
  - Extracts mnemonic and operands via Ghidra's `printAssembly` API
  - Format: `[0x401000] MOV EAX, [RBP-0x10]` displayed above each Pcode op
- **Color-Coded Nodes**: Operations grouped by type for better readability
  - Control Flow (Branch, Call, Return): Light Red (#ffcccc)
  - Memory Access (Load, Store): Light Green (#ccffcc)
  - Data Movement (Copy, Cast): White (#ffffff)
  - Arithmetic/Logic: Light Blue (#ccccff)
- **C++ Backend Enhancements**:
  - Added `run_decompilation_pcode()` function to extract raw Pcode as JSON
  - Serializes basic blocks with operations, varnodes, and assembly info
  - Fixed runtime errors with proper `Funcdata` initialization (`fd->clear()` + `fd->followFlow()`)
- **Rust FFI Integration**:
  - Added `get_pcode()` method to `RecommendedDecompiler`
  - Extended `PcodeOp` struct with `asm_mnemonic` field
  - Updated Pcode optimizer rules to preserve assembly information (7 fix locations)
- **Memory Management Fixes**:
  - Fixed "Could not find op at target address" error by adding section registration
  - All binary sections (`.text`, `.data`, etc.) now properly registered with decompiler
  - `SectionAwareLoadImage` correctly maps virtual addresses to file offsets
- **Interactive Mode Support**: Graph generation available in both oneshot and interactive CLI modes
- **Data Flow Analysis**: Optional def-use chain visualization with dotted blue edges

### Pcode IR Optimizer Phase 3
- **Common Subexpression Elimination (CSE)**: Implemented hash-based local CSE to remove redundant computations
- **RulePtrArith**: Pointer arithmetic optimization (associativity)
  - Example: `(base + 10) + 20 => base + 30`
- **RulePullSubIndirect**: Complex address calculation simplification
  - Example: `(ptr + off) - ptr => off`
- **RuleIndirectCollapse**: Indirect calculation simplification
  - Example: `PTRSUB(PTRSUB(base, 10), 20) => PTRSUB(base, 30)`
- **Test Coverage**: Added 4 new test cases covering CSE and new rules (100% passing)

### Pcode IR Optimizer Phase 2 (Commit: 3cdad8d)
- **Def-Use Tracking Infrastructure**: Implemented comprehensive def-use chain tracking with VarnodeId and DefUseInfo (370 lines)
- **NZMask Analysis**: Added non-zero mask computation for value range tracking and intelligent optimization
- **Consume Mask**: Backward propagation analysis to identify which bits are actually used by downstream operations
- **RuleShiftBitops**: Optimizes shifts that eliminate all non-zero bits
  - Example: `(V & 0xf000) << 20 => #0` (all bits shifted out of 32-bit range)
  - Supports INT_LEFT, INT_RIGHT, INT_SRIGHT operations
- **RuleAndMask**: AND operations optimized using NZMask analysis
  - Example: `V & 0xff => V` when V's NZMask is 0x0f (no-op AND elimination)
  - Example: `V & 0xf0 => #0` when V's NZMask is 0x0f (no overlapping bits)
- **Test Coverage**: 19 comprehensive tests covering all Phase 1 and Phase 2 rules (100% passing)
- **Optimizer Statistics**: 1,765 lines of code, 32 rules (~23% of Ghidra's 142 rules)

### Architecture Migration
- **Pool → FFI**: Migrated from subprocess pool to direct FFI integration via CXX bridge
- **Zero-Copy Decompilation**: Eliminated IPC overhead with native C++ bindings
- **Performance**: Significant reduction in decompilation latency and memory usage

### Documentation Updates (Commit: 8131755)
- Updated README with FFI architecture details and performance optimization section
- Added decompilation pipeline diagram showing Pcode optimization flow
- Documented Pcode optimizer Phase 1 and Phase 2 implementation details

---

## Decompiler & Analysis

### Decompiler Modularization (Commit: 85d4d3e)
- **Modular Architecture**: Refactored monolithic decompiler into clean component structure
- **GCC/MinGW FID Support**: Added Function ID database support for GCC and MinGW compilers
- **FID Coverage**: 10 database files covering VS2012-2019 (x86/x64) and legacy Windows SDK versions
- **Hash Algorithm**: Corrected FID hash implementation to FNV-1a (Commit: 9f195c4)
- **FIDBF Storage**: Fixed binary format parser for Ghidra Function ID databases

### Advanced Type Analysis (Commit: 23b565c, 1fe387a)
- **Phase 17 & 18**: Implemented advanced type analysis and output polish
- **StructureAnalyzer**: Enhanced with advanced field detection and type inference
- **Field Detection**: Automatic float/double field recognition via FPU instruction analysis
- **Critical Fixes**: Resolved structural flaws in StructureAnalyzer (Commit: cfc773a)

### FFI Integration (Commit: 8ee67fd, a2f5a5b)
- **Native Decompiler FFI**: Direct C++ integration via libdecomp (eliminated gRPC overhead)
- **Crash Fix**: Resolved FFI crash during decompilation and exit scenarios
- **Zero-Copy**: Eliminated IPC overhead with native C++ bindings

### Decompiler Pipeline (Commit: 6e71c17, 4cb838d)
- **Critical Bug Fixes**: Resolved bugs in decompiler pipeline
- **BinaryReader Utility**: Extracted common binary reading logic
- **Build System**: Improved build system and CI integration
- **Timeout Fix**: Disabled problematic Step 4b to fix decompiler timeout (Commit: b3f1fd0)
- **Re-enabled Step 4b**: Fixed StructureAnalyzer and re-enabled (Commit: 4f10c7e)

---

## CLI & UI Improvements

### CLI Enhancements (Commit: 8f46899, 026bae4)
- **One-Shot Mode**: Refactored into modular structure with dedicated command handlers
- **Command Separation**: Split analysis, decompilation, and function listing into focused modules
- **Documentation**: Comprehensive CLI one-shot mode guide (Commit: 56195f6)
- **Flag Updates**: Added new CLI flags with updated README documentation (Commit: 277a798)
- **Error Handling**: Improved error messages and user feedback
- **CLI v0.2.0**: Added Sections, Imports, Disasm views with robust I/O (Commit: eccfdda)

### GUI Refactoring (Commit: b7a29a4, 1c37532)
- **Module Split**: Split large GUI modules into focused files
- **Debug Panel**: UI overhaul for debug panel
- **Stability**: Improved UI stability and responsiveness
- **Tabbed Panels**: Console, Hex View, Strings in tabbed interface (Commit: 87f3e8a)
- **x64dbg-Style View**: Added x64dbg-inspired assembly view (Commit: 0798c94)

### Code Organization (Commit: 41b02d1, 0dbbd22)
- **TUI Refactoring**: Reorganized TUI into modular folder structure
- **CLI Unification**: Reorganized CLI code into unified src/cli/ module
- **Large File Split**: Split large files into modular structure for maintainability
- **UI Patterns**: Extracted common empty state UI pattern into helper function (Commit: 506f2da)

---

## Performance & Optimization

### Code Quality & Performance (Commit: 12f3e03, f70584f, 7866ca2)
- **Clippy Fixes**: Comprehensive code quality improvements across codebase
- **LazyLock Migration**: Replaced lazy_static with modern LazyLock for better performance
- **Type Safety**: Enhanced type safety throughout the project
- **String Extraction**: Optimized with pre-allocation for faster performance
- **Disassembly**: Performance improvements with buffer pre-allocation

### Cross-Reference & Loader Optimization (Commit: 9e27da8, 6184208, b3e47ef)
- **XRef Performance**: Improved cross-reference analysis speed
- **Loader Types**: Enhanced binary loader type handling
- **Benchmarks**: Added performance benchmarks for critical paths
- **Function Discovery**: Removed unnecessary sorting for O(1) lookups
- **Helper Functions**: Extracted common patterns to reduce duplication
- **Analysis Module**: Performance improvements across analysis components
- **UI Module**: Optimized UI rendering and updates

### Code Refactoring (Commit: f481c85, ed62681)
- **String Extraction**: Refactored duplicated code into shared utilities
- **Overflow Safety**: Added checked_add for arithmetic overflow protection

---

## Debugging & Dynamic Analysis

### Time Travel Debugging (Commit: 1813814, 341631a, 593af70)
- **TTD Optimization**: Performance improvements in critical code paths
- **Signature Optimization**: Enhanced signature matching performance
- **Snapshot Management**: Improved TTD snapshot handling
- **TTD Implementation**: Full time travel debugging support
- **Windows TTD**: Time Travel Debugging integration for Windows

### Titan Debug Engine (Commit: b80d79d)
- **New Debug Engine**: Added Titan debug engine for advanced debugging
- **Parser Modularization**: Split parsers into modular components

### Debugger Module (Commit: 661d11c)
- **Platform-Specific APIs**: Implemented Windows and Linux debugger APIs
- **Abstraction Layer**: Created platform-agnostic debugger interface

### Cross-References & Features (Commit: 4b0ebfc, 815d46d, 8e28314)
- **Xref System**: Implemented code and data cross-reference analysis
- **Binary Detector**: DiE-style packer and compiler detection
- **Binary Patching**: Added binary patching for crackme analysis
- **Memory Modification**: Live memory patching during debugging

---

## Signatures & Type System

### Windows API Database (Commit: 9577508, f1140ea, 3791a98)
- **100+ API Mappings**: Expanded Windows API signatures database
- **High-Priority APIs**: Added kernel32, ntdll, services APIs
- **HTTP & Shell**: Added WinHTTP, shell32, bcrypt APIs (50+ new)
- **Extended User32**: Enhanced user32 API coverage

### Signature Database Expansion (Commit: fd2c9b6, 2227a65, 401cd80)
- **Advanced Signatures**: Added syscall, injection, packer detection
- **C++ Analysis**: Enhanced C++ class and virtual table detection
- **Anti-Debug**: Added anti-debugging technique signatures
- **Crypto & Compression**: Added cryptography and compression signatures
- **x86/MinGW**: Added x86-specific and MinGW compiler signatures
- **WinHTTP & Registry**: Added HTTP and registry operation signatures

### CRT Signatures (Commit: 4baf99f, 0671969)
- **40+ CRT Functions**: Expanded C runtime function signature database
- **x64 CRT**: Enhanced x64-specific CRT signature coverage

### Windows Structures (Commit: 74e0da9, 4302da1, 55de079, f1d7f3c)
- **30+ Advanced Structures**: Added TLS, NT internals, Delay Import structures
- **Architecture-Specific**: Refined x86/x64 structure definitions
- **Security Structures**: Added security descriptor and token structures
- **ToolHelp32**: Added process/module enumeration structures
- **Exception Handling**: Added SEH and exception record structures
- **PE Headers**: Complete PE format structure definitions
- **Network Structures**: Added socket and networking structures
- **GUI Structures**: Added window, message, and GDI structures
- **Memory Structures**: Added heap, memory descriptor structures
- **Loader Structures**: Added module and import table structures
- **Korean Comment Removal**: Cleaned up Korean comments (Commit: 45ffb2c)

### Data Types Module (Commit: 220d7cf)
- **Windows Data Types**: Comprehensive Windows type definitions module
- **Type Compatibility**: Ensured cross-platform type compatibility

### IAT & Symbol Injection (Commit: b769cc6, 85104ab, 55b4c61)
- **IAT Post-Processing**: Indirect call resolution through Import Address Table
- **Ghidra Options**: Added advanced Ghidra decompiler options
- **Symbol Injection**: Automatic symbol injection for better decompilation
- **ELF/Mach-O Symbols**: Enhanced symbol extraction for Unix binaries
- **Function Rename UI**: Added UI for manual function renaming

---

## Plugin System & Extensibility

### Plugin Architecture (Commit: 0b5e168, df4eef0, b2c12f5)
- **FissionPlugin Trait**: Implemented comprehensive plugin trait system
- **Builder Pattern**: Added builder pattern for clean initialization
- **Event Bus**: Event-driven architecture for plugin communication
- **Command Pattern**: Structured command handling system
- **PyO3 Plugins**: Python plugin support via PyO3

### Python Scripting (Commit: 31b4e3d, 0ccd396)
- **Enhanced API**: Improved Python scripting API
- **Script Panel**: Added dedicated scripting panel
- **Function Metadata**: Cache function metadata for performance (Commit: 9e44d4e)

---

## Infrastructure & Build System

### CI/CD Pipeline (Commit: b406634, 63865a9)
- **Full CI/CD Setup**: Comprehensive pipeline with security, testing, and deployment
- **CodeQL v4**: Upgraded to CodeQL actions v4 for security analysis
- **Trivy SARIF**: Configured container scanning with SARIF output
- **Windows Build**: Added vcpkg zlib installation for Windows CI (Commit: 78f0c3f)
- **CMake Action**: Removed deprecated jwlawrence/cmake-action (Commit: 5fc8faa)
- **Coverage CI**: Added coverage workflow with grcov (Commit: 2662ae8)

### Testing & Quality Assurance (Commit: 973374d, 63865a9)
- **Proptest Integration**: Property-based testing for robustness
- **Insta Snapshots**: Snapshot testing for regression detection
- **Stricter Clippy**: Enhanced linting rules for code quality
- **Doctest Fixes**: Resolved compilation errors in core module (Commit: 5fc8faa)

### Core Utilities (Commit: 7ea1bdd, 3622f8a, 4ccba79)
- **Module Organization**: Moved utilities to src/core/ folder
- **Constants Module**: Centralized magic bytes and offsets
- **Logging Utility**: Added structured logging module
- **Prelude**: Added prelude.rs for common imports (Commit: fc84d5f)
- **Error Handling**: Comprehensive error handling module (Commit: fcd174d)
- **Configuration**: Centralized config.rs (Commit: f103273, a1645c2)

### Platform Abstraction (Commit: 355c108, be73f09)
- **Code Quality**: Platform abstraction layer improvements
- **Logging Unification**: Centralized logging across modules
- **Test Expansion**: Expanded test coverage for core components
- **Timeout Resolution**: Fixed decompiler timeout with image_base support
- **PE Memory Mapping**: Added proper PE file memory mapping

---

## Architecture Evolution

### Architectural Upgrades (Commit: 7bc1bd7, 4f24f03)
- **Major Refactoring**: Comprehensive architectural improvements
- **README Overhaul**: Complete documentation rewrite (Commit: 0daa2be)
- **Major Structural Improvements**: Better separation of concerns

### Project Restructure (Commit: d51fe0c, 6dc52fe)
- **Major Restructure**: Complete project reorganization for extensibility
- **GUI/CLI Separation**: Separated GUI and CLI into distinct modules

### Binary Loader (Commit: de3d9be, 6ed8dfb, b251b71)
- **Multi-Format**: PE/ELF/Mach-O binary loader module
- **Format Detection**: Automatic binary format detection
- **Enhanced Error Handling**: Custom error types
- **Path Resolution**: Dynamic executable-relative path resolution

### Server Mode & Detection (Commit: 312ce06, 78eaffd)
- **Server Mode**: Preparation for decompiler server architecture
- **Memory Corruption Fix**: Resolved server mode memory issues
- **PyInstaller Detection**: Added packed executable detection

---

## Ghidra Integration History

### gRPC Architecture (Commit: 03d4bee, 354d75b)
- **gRPC Integration**: Complete gRPC-based Ghidra decompiler integration
- **Documentation**: Updated README with gRPC architecture details
- **Protocol Optimization**: Full function analysis with CFG/Assembly in one call (Commit: c797f50, 1bd1330)

### C++ Wrapper & FFI Bridge (Commit: 51d1343, afc3750, dc60381)
- **C++ Wrapper Fix**: Fixed crash in C++ wrapper (simplified without Ghidra init)
- **Phase 2 Complete**: Ghidra C++ decompiler API integration with vcpkg zlib
- **FFI Bridge**: Ghidra decompiler FFI integration with stub fallback
- **Removed iced-x86**: Replaced with Ghidra C++ source

### SLEIGH Language Specs (Commit: 466a630, 9a9907a)
- **x86 Support**: Added x86 and x86-64 .sla files
- **Renamed Folder**: cpp/ → ghidra_decompiler/ for clarity

---

## .NET & Binary Format Support

### .NET Support (Commit: 340c3de, f6aedf7)
- **CLR Detection**: .NET binary detection and analysis
- **iced-x86**: Integrated iced-x86 pure Rust disassembler
- **IL Disassembly**: .NET Intermediate Language disassembly
- **Debug Features**: Enhanced debugging capabilities

### Binary Loader & Format Detection (Commit: de3d9be, 0798c94)
- **Multi-Format**: PE/ELF/Mach-O binary loader module
- **Format Detection**: Automatic binary format detection
- **PE Loading**: Improved PE binary loading
- **Ghidra Stability**: Stabilized Ghidra server connection

---

## Project Foundation

### Dependencies (Commit: 32983fe, b566124)
- **PyO3 Bump**: Updated pyo3 from 0.21.2 to 0.24.1 via Dependabot

### Project Scaffolding (Commit: 7e66807)
- **Phase 1**: Complete project scaffolding (November 2025)

---

## Statistics

- **Optimizer**: 32 optimization rules (~23% of Ghidra's 142 rules)
- **Code Base**: 1,765 lines in optimizer module alone
- **Test Coverage**: 19 passing tests with comprehensive validation
- **Platform Support**: Windows, Linux, and macOS
- **API Database**: 100+ Windows API mappings across 9 DLLs
- **Structures**: 5,700+ structures and 6,500+ typedefs from Ghidra GDT
- **Signatures**: 40+ CRT functions, advanced packer/anti-debug detection
- **Total Commits**: 150+ commits tracking feature development and improvements
- **Project Duration**: November 2025 - January 2026 (Current)
