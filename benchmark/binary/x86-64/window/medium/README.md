# Windows x86-64 Medium Test Binaries

This directory contains multi-language Windows x86-64 decompilation test binaries for medium-sized benchmarking.

## Directory Structure

```
.
├── README.md                                    # This file
├── build.sh                                     # Master build script
├── source/                                      # Source code directory
│   ├── c/                                       # C source (complex algorithms)
│   │   ├── algorithms.c
│   │   └── build.sh
│   ├── cpp/                                     # C++ source (OOP & templates)
│   │   ├── classes_templates.cpp
│   │   └── build.sh
│   ├── golang/                                  # Go source (concurrency)
│   │   ├── algorithms.go
│   │   └── build.sh
│   └── rustlang/                                # Rust source (ownership & traits)
│       ├── algorithms.rs
│       └── build.sh
└── binary/                                      # Compiled binaries directory
    ├── c/
    ├── cpp/
    ├── golang/
    └── rustlang/
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

## Medium Binary Characteristics

**Size**: 1-5 MB (larger than small, suitable for benchmarking performance)

**Complexity**: Includes complex algorithms, OOP patterns, and language-specific features

### C (algorithms.c)
- **Algorithms**: Dijkstra's, DFS, Floyd-Warshall graph algorithms
- **String Processing**: KMP, Rabin-Karp pattern matching
- **Sorting**: Merge sort, Quick sort
- **Encryption**: Simple stream cipher
- **Expected Size**: 200-500 KB

### C++ (classes_templates.cpp)
- **Templates**: Generic Stack, HashMap
- **OOP**: Shape hierarchy (Circle, Rectangle, Triangle)
- **Advanced**: Binary Search Tree with shared pointers
- **STL**: Vector, Map, Algorithm
- **Expected Size**: 300-700 KB

### Go (algorithms.go)
- **Concurrency**: Worker pool pattern with goroutines
- **Data Structures**: Linked List, Binary Search Tree
- **Algorithms**: QuickSort, MergeSort
- **String Processing**: Pattern matching, Palindrome detection
- **Expected Size**: 2-4 MB (Go runtime included)

### Rust (algorithms.rs)
- **Traits**: Drawable, Comparable trait implementations
- **Generics**: Generic Stack, TreeNode
- **Ownership**: Proper memory management
- **Advanced**: Pattern matching, Option/Result types
- **Expected Size**: 1-3 MB

## Building Requirements

### macOS
```bash
# Install MinGW-w64 for C/C++
brew install mingw-w64

# Go and Rust should already be installed
# Install Rust Windows target
rustup target add x86_64-pc-windows-gnu
```

### Linux
```bash
# Install MinGW-w64
sudo apt-get install mingw-w64

# Install Go (if not present)
# Install Rust Windows target
rustup target add x86_64-pc-windows-gnu
```

## File Sizes Reference

After compilation, expected binary sizes:

```
c/algorithms.exe              ~250-400 KB
cpp/classes_templates.exe     ~400-600 KB
golang/algorithms.exe         ~2-4 MB
rustlang/algorithms.exe       ~1-3 MB
```

Total expected: **4-8 MB** for all languages combined

## Usage in Benchmarking

These binaries are used for:

1. **Performance Testing**: Medium-sized decompilation tasks (5-10 seconds per binary)
2. **Algorithm Decompilation**: Complex algorithms to analyze
3. **Language Coverage**: Multi-language support validation
4. **Scalability Testing**: Between small (< 1MB) and large (> 10MB) binaries

### Example Commands

```bash
# Build medium binaries
./build.sh all

# Decomp with Fission CLI
fission_cli decomp binary/c/algorithms.exe --addr 0x140001450
fission_cli decomp binary/cpp/classes_templates.exe --list --json
fission_cli decomp binary/golang/algorithms.exe --function
```

## Notes

- Binaries are statically linked to avoid runtime dependencies
- Debug symbols (-g) are included for better analysis
- Optimization level -O2 is used for realistic performance profiles
- Binaries are stripped of unnecessary symbols but retain debug info
