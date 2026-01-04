# Changelog

All notable changes to the Fission project.

---

## January 2026

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
- Added Recent Updates section (January 2026)

---

## December 2025

### Decompiler Modularization (Commit: 85d4d3e)
- **Modular Architecture**: Refactored monolithic decompiler into clean component structure
- **GCC/MinGW FID Support**: Added Function ID database support for GCC and MinGW compilers
- **FID Coverage**: 10 database files covering VS2012-2019 (x86/x64) and legacy Windows SDK versions
- **Hash Algorithm**: Corrected FID hash implementation to FNV-1a (Commit: 9f195c4)
- **FIDBF Storage**: Fixed binary format parser for Ghidra Function ID databases

### CLI Enhancements (Commit: 8f46899, 026bae4)
- **One-Shot Mode**: Refactored into modular structure with dedicated command handlers
- **Command Separation**: Split analysis, decompilation, and function listing into focused modules
- **Documentation**: Comprehensive CLI one-shot mode guide (Commit: 56195f6)
- **Flag Updates**: Added new CLI flags with updated README documentation (Commit: 277a798)
- **Error Handling**: Improved error messages and user feedback

### Code Organization (Commit: 41b02d1, 0dbbd22)
- **TUI Refactoring**: Reorganized TUI into modular folder structure
- **CLI Unification**: Reorganized CLI code into unified src/cli/ module
- **Large File Split**: Split large files into modular structure for maintainability

---

## November 2025

### FFI Integration & Performance (Commit: 8ee67fd, a2f5a5b)
- **Native Decompiler FFI**: Direct C++ integration via libdecomp (eliminated gRPC overhead)
- **Crash Fix**: Resolved FFI crash during decompilation and exit scenarios
- **Enhanced CLI/TUI**: Binary analysis improvements (Commit: c3b8582)
- **CLI v0.2.0**: Added Sections, Imports, Disasm views with robust I/O (Commit: eccfdda)

### Code Quality & Performance (Commit: 12f3e03, f70584f, 7866ca2)
- **Clippy Fixes**: Comprehensive code quality improvements across codebase
- **LazyLock Migration**: Replaced lazy_static with modern LazyLock for better performance
- **Type Safety**: Enhanced type safety throughout the project
- **String Extraction**: Optimized with pre-allocation for faster performance
- **Disassembly**: Performance improvements with buffer pre-allocation

### Cross-Reference & Loader Optimization (Commit: 9e27da8)
- **XRef Performance**: Improved cross-reference analysis speed
- **Loader Types**: Enhanced binary loader type handling
- **Benchmarks**: Added performance benchmarks for critical paths

### Code Refactoring (Commit: f481c85, ed62681)
- **String Extraction**: Refactored duplicated code into shared utilities
- **Overflow Safety**: Added checked_add for arithmetic overflow protection
- **UI Patterns**: Extracted common empty state UI pattern into helper function (Commit: 506f2da)

---

## October 2025

### Time Travel Debugging (Commit: 1813814, 341631a)
- **TTD Optimization**: Performance improvements in critical code paths
- **Signature Optimization**: Enhanced signature matching performance
- **Snapshot Management**: Improved TTD snapshot handling

### Architectural Upgrades (Commit: 7bc1bd7)
- **Major Refactoring**: Comprehensive architectural improvements
- **README Overhaul**: Complete documentation rewrite (Commit: 0daa2be)

### Advanced Type Analysis (Commit: 23b565c, 1fe387a)
- **Phase 17 & 18**: Implemented advanced type analysis and output polish
- **StructureAnalyzer**: Enhanced with advanced field detection and type inference
- **Field Detection**: Automatic float/double field recognition via FPU instruction analysis
- **Critical Fixes**: Resolved structural flaws in StructureAnalyzer (Commit: cfc773a)

### Testing & Quality Assurance (Commit: 973374d, 63865a9)
- **Proptest Integration**: Property-based testing for robustness
- **Insta Snapshots**: Snapshot testing for regression detection
- **Stricter Clippy**: Enhanced linting rules for code quality
- **Doctest Fixes**: Resolved compilation errors in core module (Commit: 5fc8faa)

---

## September 2025

### CI/CD Pipeline (Commit: b406634, 63865a9)
- **Full CI/CD Setup**: Comprehensive pipeline with security, testing, and deployment
- **CodeQL v4**: Upgraded to CodeQL actions v4 for security analysis
- **Trivy SARIF**: Configured container scanning with SARIF output
- **Windows Build**: Added vcpkg zlib installation for Windows CI (Commit: 78f0c3f)
- **CMake Action**: Removed deprecated jwlawrence/cmake-action (Commit: 5fc8faa)

### Performance Optimizations (Commit: 6184208, b3e47ef)
- **Function Discovery**: Removed unnecessary sorting for O(1) lookups
- **Helper Functions**: Extracted common patterns to reduce duplication
- **Analysis Module**: Performance improvements across analysis components
- **UI Module**: Optimized UI rendering and updates

### Decompiler Pipeline (Commit: 6e71c17, 4cb838d)
- **Critical Bug Fixes**: Resolved bugs in decompiler pipeline
- **BinaryReader Utility**: Extracted common binary reading logic
- **Build System**: Improved build system and CI integration
- **Timeout Fix**: Disabled problematic Step 4b to fix decompiler timeout (Commit: b3f1fd0)
- **Re-enabled Step 4b**: Fixed StructureAnalyzer and re-enabled (Commit: 4f10c7e)

---

## August 2025

### Titan Debug Engine (Commit: b80d79d)
- **New Debug Engine**: Added Titan debug engine for advanced debugging
- **Parser Modularization**: Split parsers into modular components
- **CLI Refactoring**: Improved command-line interface structure

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

---

## July 2025

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

---

## June 2025

### IAT & Symbol Injection (Commit: b769cc6, 85104ab)
- **IAT Post-Processing**: Indirect call resolution through Import Address Table
- **Ghidra Options**: Added advanced Ghidra decompiler options
- **Symbol Injection**: Automatic symbol injection for better decompilation
- **Windows Types**: Enhanced Windows type definitions
- **CRT Signatures**: Added C runtime function signatures
- **Race Condition Fix**: Resolved threading issues in decompiler

### Phase 2 Features (Commit: 55b4c61)
- **CRT Signatures**: Added C runtime function identification
- **ELF/Mach-O Symbols**: Enhanced symbol extraction for Unix binaries
- **Function Rename UI**: Added UI for manual function renaming
- **Windows API DB**: Comprehensive Windows API database integration

---

## May 2025

### Platform Abstraction (Commit: 355c108)
- **Code Quality**: Platform abstraction layer improvements
- **Logging Unification**: Centralized logging across modules
- **Test Expansion**: Expanded test coverage for core components

### Debugger Module (Commit: 661d11c)
- **Platform-Specific APIs**: Implemented Windows and Linux debugger APIs
- **Abstraction Layer**: Created platform-agnostic debugger interface

### Decompiler Fixes (Commit: be73f09, 9b2b103)
- **Timeout Resolution**: Fixed decompiler timeout with image_base support
- **PE Memory Mapping**: Added proper PE file memory mapping
- **Clippy Warnings**: Fixed all clippy warnings
- **Legacy Code**: Removed obsolete code paths

---

## April 2025

### Plugin System (Commit: 0b5e168, df4eef0)
- **FissionPlugin Trait**: Implemented comprehensive plugin trait system
- **Builder Pattern**: Added builder pattern for clean initialization
- **Event Bus**: Event-driven architecture for plugin communication
- **Command Pattern**: Structured command handling system

### Core Utilities (Commit: 7ea1bdd, 3622f8a, 4ccba79)
- **Module Organization**: Moved utilities to src/core/ folder
- **Constants Module**: Centralized magic bytes and offsets
- **Logging Utility**: Added structured logging module
- **Prelude**: Added prelude.rs for common imports (Commit: fc84d5f)

### Error Handling (Commit: fcd174d)
- **Unified Errors**: Comprehensive error handling module
- **Error Types**: Defined domain-specific error types

### Configuration (Commit: f103273, a1645c2)
- **Centralized Config**: Moved hardcoded values to config.rs
- **Multi-Process Pool**: Decompiler pool configuration

---

## March 2025

### Server Mode & Detection (Commit: 312ce06, 78eaffd)
- **Server Mode**: Preparation for decompiler server architecture
- **Memory Corruption Fix**: Resolved server mode memory issues
- **PyInstaller Detection**: Added packed executable detection

### Cross-References (Commit: 4b0ebfc)
- **Xref System**: Implemented code and data cross-reference analysis
- **UI Icon Fixes**: Fixed icon rendering issues in UI

### Binary Detector (Commit: 815d46d)
- **DiE-Style Detection**: Binary packer and compiler detection
- **Debug UI**: Improved debug panel interface
- **Plugins Sidebar**: Added dedicated plugins management sidebar

### Binary Patching (Commit: 8e28314)
- **Patch Feature**: Added binary patching for crackme analysis
- **Memory Modification**: Live memory patching during debugging

---

## February 2025

### TTD & Python Integration (Commit: 8e28314, b2c12f5)
- **Windows TTD**: Time Travel Debugging integration for Windows
- **PyO3 Plugins**: Python plugin support via PyO3
- **Plugin System**: Extensible plugin architecture

### Time Travel Debugging (Commit: 593af70)
- **TTD Implementation**: Full time travel debugging support
- **Decompiler Improvements**: Enhanced decompiler output quality

### Major Structural Improvements (Commit: 4f24f03)
- **Architecture Refactoring**: Major project structure improvements
- **Module Organization**: Better separation of concerns

### Python Scripting (Commit: 31b4e3d, 0ccd396)
- **Enhanced API**: Improved Python scripting API
- **Script Panel**: Added dedicated scripting panel
- **FFI Migration**: Removed gRPC, switched to native FFI (Commit: 0ccd396)

---

## January 2025

### .NET Support (Commit: 340c3de, f6aedf7)
- **CLR Detection**: .NET binary detection and analysis
- **iced-x86**: Integrated iced-x86 pure Rust disassembler
- **IL Disassembly**: .NET Intermediate Language disassembly
- **Debug Features**: Enhanced debugging capabilities

### GUI Refactoring (Commit: b7a29a4, 1c37532)
- **Module Split**: Split large GUI modules into focused files
- **Debug Panel**: UI overhaul for debug panel
- **Stability**: Improved UI stability and responsiveness

### Function Metadata & Caching (Commit: 9e44d4e)
- **Metadata Caching**: Cache function metadata for performance
- **Client Reconnects**: Stabilized client reconnection handling

### Coverage & Testing (Commit: 2662ae8)
- **Coverage CI**: Added coverage workflow with grcov
- **Tabbed Panels**: Console, Hex View, Strings in tabbed interface (Commit: 87f3e8a)

### GUI Improvements (Commit: eeecf4f, b5c03e6)
- **Documentation**: Updated README with latest features and modular structure
- **Panel Modularization**: Separated GUI into distinct panel components
- **x64dbg-Style View**: Added x64dbg-inspired assembly view (Commit: 0798c94)
- **PE Loading**: Improved PE binary loading
- **Ghidra Stability**: Stabilized Ghidra server connection

---

## December 2024

### Project Restructure (Commit: d51fe0c, 6dc52fe)
- **Major Restructure**: Complete project reorganization for extensibility
- **GUI/CLI Separation**: Separated GUI and CLI into distinct modules

### Binary Loader (Commit: de3d9be)
- **Multi-Format**: PE/ELF/Mach-O binary loader module
- **Format Detection**: Automatic binary format detection

### Error Handling Enhancement (Commit: 6ed8dfb)
- **Custom Error Types**: Enhanced error handling with domain-specific types
- **Path Resolution**: Dynamic executable-relative path resolution (Commit: b251b71)

---

## November 2024

### gRPC Architecture (Commit: 03d4bee, 354d75b)
- **gRPC Integration**: Complete gRPC-based Ghidra decompiler integration
- **Documentation**: Updated README with gRPC architecture details

### Protocol Optimization (Commit: c797f50, 1bd1330)
- **Single-Call Analysis**: Full function analysis with CFG/Assembly in one call
- **gRPC Implementation**: Server and client implementation

---

## October 2024

### Ghidra Integration (Commit: 51d1343, afc3750)
- **C++ Wrapper**: Fixed crash in C++ wrapper (simplified without Ghidra init)
- **Phase 2**: Complete Ghidra C++ decompiler API integration with vcpkg zlib

### SLEIGH Language Specs (Commit: 466a630, 9a9907a)
- **x86 Support**: Added x86 and x86-64 .sla files
- **Renamed Folder**: cpp/ → ghidra_decompiler/ for clarity

### FFI Bridge (Commit: dc60381)
- **Phase 2**: Ghidra decompiler FFI integration
- **Removed iced-x86**: Replaced with Ghidra C++ source
- **Stub Fallback**: Implemented FFI bridge with fallback

---

## September 2024

### Dependencies (Commit: 32983fe, b566124)
- **PyO3 Bump**: Updated pyo3 from 0.21.2 to 0.24.1 via Dependabot

### Project Foundation (Commit: 7e66807)
- **Phase 1**: Complete project scaffolding

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
