# Windows x86-64 Small Test Binaries

This directory contains multiple multi-language Windows x86-64 decompilation test binaries for benchmarking purposes.

## Directory Structure

```
.
├── README.md                                    # This file
├── build.sh                                     # Master build script (builds all languages)
├── source/                                      # Source code directory
│   ├── c/                                       # C source code (compiled with Clang)
│   │   ├── *.c
│   │   └── build.sh
│   ├── cpp/                                     # C++ source code (compiled with Clang++)
│   │   ├── *.cpp
│   │   └── build.sh
│   ├── golang/                                  # Go source code
│   │   ├── *.go
│   │   └── build.sh
│   └── rustlang/                                # Rust source code
│       ├── *.rs
│       └── build.sh
└── binary/                                      # Compiled binaries directory
    ├── c/                                       # C binaries (.exe)
    ├── cpp/                                     # C++ binaries (.exe)
    ├── golang/                                  # Go binaries (.exe)
    └── rustlang/                                # Rust binaries (.exe)
```

## Quick Start

### Build All Languages

```bash
./build.sh
```

### Build Specific Language

```bash
./build.sh c              # Build C only
./build.sh cpp            # Build C++ only
./build.sh golang         # Build Go only
./build.sh rust           # Build Rust only
./build.sh all            # Build all languages
```

## External Evaluation Starter Set

If you are evaluating Fission from the CLI, use this directory as the first checked-in Windows x86-64 sample surface.

Recommended first binaries:

- `binary/c/test_functions.exe`
  - clean first pass for `info`, `list`, `disasm`, and `decomp`
- `binary/c/structs_and_pointers.exe`
  - pointer and aggregate surfacing
- `binary/c/bitops_and_control_flow.exe`
  - branch-heavy and bit-operation-heavy logic
- `binary/c/function_pointers_and_strings.exe`
  - strings and indirect-style patterns

Recommended first function for `test_functions.exe`:

- `0x140001450` (`add`)

Canonical external evaluator flow:

```bash
./target/release/fission_cli info benchmark/binary/x86-64/window/small/binary/c/test_functions.exe
./target/release/fission_cli list benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --json
./target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450 --function
./target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450
./target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450 --json
```

For the full evaluator-oriented guide, see [docs/EVALUATION.md](../../../../../docs/EVALUATION.md).

## Language-Specific Details

### C Language (source/c/)

**Files**: 
- `test_functions.c` - Basic C functions
- `structs_and_pointers.c` - Struct and pointer operations
- `bitops_and_control_flow.c` - Bit operations and control flow
- `function_pointers_and_strings.c` - Function pointers and strings
- `math_operations.c` - Mathematical operations
- `array_operations.c` - Array and sorting algorithms

**Compiler**: MinGW-w64 (x86_64-w64-mingw32-gcc)

**Flags**: `-O2 -g -m64 -static -lm`

**Build**:
```bash
./source/c/build.sh
```

### C++ Language (source/cpp/)

**Files**: Same as C but with C++ implementations:
- `test_functions.cpp` - Classes, templates, and virtual functions
- `structs_and_pointers.cpp` - Class design, shared_ptr, RAII
- `bitops_and_control_flow.cpp` - Enums, state machines, templates
- `function_pointers_and_strings.cpp` - Lambdas, std::string, STL algorithms
- `math_operations.cpp` - Template classes, static methods
- `array_operations.cpp` - Template classes, STL containers, algorithms

**Compiler**: MinGW-w64 (x86_64-w64-mingw32-g++)

**Flags**: `-O2 -g -m64 -static -lm -std=c++17`

**Build**:
```bash
./source/cpp/build.sh
```

**C++ Features Used**:
- Object-oriented programming (classes, inheritance, polymorphism)
- Templates (generic programming)
- STL containers (vector, string)
- Smart pointers (shared_ptr, make_shared)
- Lambdas and functional programming
- Exception handling
- Standard algorithms (sort, find, transform, etc.)

### Go Language (source/golang/)

**Files**: Same pattern as C and C++:
- `test_functions.go` - Basic functions, memoization, slices
- `structs_and_pointers.go` - Struct definitions, receiver methods, linked lists
- `bitops_and_control_flow.go` - Bit operations, state machines, complex control flow
- `function_pointers_and_strings.go` - Function types, string operations, functional patterns
- `math_operations.go` - Number theory, geometry, statistics, matrix operations
- `array_operations.go` - Sorting algorithms, binary search, array manipulation

**Compiler**: Go (cross-compilation to windows/amd64)

**Environment Variables**: `GOOS=windows GOARCH=amd64 CGO_ENABLED=1`

**Build**:
```bash
./source/golang/build.sh
```

**Go Features Used**:
- Goroutines and concurrency patterns (simulated in test code)
- Interfaces and duck typing
- Receiver methods and pointer semantics
- Slice operations and dynamic arrays
- Map-based memoization
- Standard library (fmt, math, sort)

### Rust Language (source/rustlang/)

**Files**: Same pattern as C and C++:
- `test_functions.rs` - Basic Rust functions, ownership, Option/Result types
- `structs_and_pointers.rs` - Struct definitions, impl blocks, Box/Rc/Arc memory management
- `bitops_and_control_flow.rs` - Bitwise operations, match expressions, enums for state machines
- `function_pointers_and_strings.rs` - Function pointers, closures, String operations, iterators
- `math_operations.rs` - Mathematical functions, generic impl blocks, HashMap for memoization
- `array_operations.rs` - Vector operations, sorting with custom comparators, slice operations

**Compiler**: Rust (x86_64-pc-windows-gnu target)

**Requirements**:
- Rust installed
- x86_64-pc-windows-gnu target (auto-installed by build.sh if needed)

**Build**:
```bash
./source/rustlang/build.sh
```

**Rust Features Used**:
- Ownership and borrowing system
- Pattern matching and match expressions
- Enums and Option/Result types
- Trait implementations and generic programming
- Closures and functional iterators
- HashMap for memoization
- Memory safety without garbage collection
- Error handling with Result types

## Prerequisites by Language

### macOS

```bash
# C/C++
brew install mingw-w64

# Go
brew install go

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Linux

```bash
# C/C++
sudo apt-get install mingw-w64

# Go
sudo apt-get install golang-go

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Compilation Flags

### C/C++
- `-O2`: Optimization level 2 (balance between optimization and debuggability)
- `-g`: Include debug symbols
- `-m64`: Target 64-bit architecture
- `-static`: Link statically to reduce runtime dependencies
- `-lm`: Link math library (C/C++)
- `-std=c++17`: C++ standard (C++ only)

### Go/Rust
- Cross-compilation targeting Windows x86-64
- Optimization enabled for realistic decompilation testing
- Static linking for portability

## Usage

The compiled binaries are designed for:

1. **Decompilation testing**: Test decompilation accuracy across multiple programming languages
2. **Benchmarking**: Measure decompilation performance and quality per language
3. **Quality lanes**: Validate decompiler output for language-specific patterns
4. **Comparative analysis**: Compare how well the decompiler handles different languages

## Re-building

To rebuild all binaries after modifying source files:

```bash
./build.sh
```

To rebuild specific language:

```bash
./source/<language>/build.sh
```

## Notes

- All binaries are compiled with `-O2` optimization for realistic decompilation testing
- Debug symbols are included for validation and source mapping
- Binaries are statically linked where possible to ensure portability
- Each language showcases language-specific patterns and idioms
