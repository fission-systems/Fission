# Fission Test Suite

This directory contains comprehensive C test programs designed to evaluate Fission's decompilation capabilities across various scenarios.

## Test Files

### 1. `struct_test.c` - Structure Recovery
Tests Fission's ability to recover and reconstruct structure definitions from compiled binaries.

**Key Features:**
- Nested structures
- Structure pointer handling
- Field access patterns
- Memory allocation with structures

**Compile:**
```bash
# Windows (MSVC)
cl /O2 struct_test.c

# Linux/macOS (GCC) - x86-64
gcc -O2 -o struct_test struct_test.c

# macOS (Apple Silicon) - x86-64
gcc -arch x86_64 -O2 -o struct_test_x64 struct_test.c

# Cross-compile for Windows (MinGW)
x86_64-w64-mingw32-gcc -O2 -o struct_test_x64.exe struct_test.c  # 64-bit
i686-w64-mingw32-gcc -O2 -o struct_test_x86.exe struct_test.c    # 32-bit
```

### 2. `winapi_test.c` - Windows API Function Identification
- Automatic function name recognition via FID database
Tests FID (Function ID) matching and Windows API constant resolution.

**Key Features:**
- File I/O operations (kernel32.dll)
- Memory management (VirtualAlloc, HeapAlloc)
- Registry operations (advapi32.dll)
- Network operations (ws2_32.dll)
- Message boxes and UI (user32.dll)
- Cryptography (advapi32.dll)
- Process/Thread creation

**Compile:**
```bash
# Windows (MSVC)
cl /O2 winapi_test.c advapi32.lib user32.lib ws2_32.lib

# MinGW
x86_64-w64-mingw32-gcc -O2 -o winapi_test.exe winapi_test.c -ladvapi32 -luser32 -lws2_32
```

### 3. `control_flow_test.c` - Control Flow Analysis ✨ NEW
Tests decompilation of various control flow patterns and language constructs.

**Key Features:**
- **Switch statements** - Jump table recovery
- **Nested loops** - Loop detection and structuring
- **Recursion** - Direct and tail recursion
- **Complex conditionals** - If-else chains
- **Bitwise operations** - Bit manipulation patterns
- **Pointer arithmetic** - Array manipulation
- **Function pointers** - Indirect calls
- **Variadic functions** - Variable argument lists
- **Inline assembly** - ASM block detection
- **String operations** - Memory manipulation
- **Structure arrays** - Complex data structures

**Compile:**
```bash
# Windows (MSVC)
cl /O2 control_flow_test.c

# Linux/macOS (GCC) - x86-64
gcc -O2 -o control_flow_test control_flow_test.c

# macOS (Apple Silicon) - x86-64
gcc -arch x86_64 -O2 -o control_flow_test_x64 control_flow_test.c

# Cross-compile for Windows (MinGW)
x86_64-w64-mingw32-gcc -O2 -o control_flow_test_x64.exe control_flow_test.c  # 64-bit
i686-w64-mingw32-gcc -O2 -o control_flow_test_x86.exe control_flow_test.c    # 32-bit
```

### 4. `datatype_test.c` - Data Type Analysis ✨ NEW
Tests Fission's type inference and data structure recognition.

**Key Features:**
- **Integer boundaries** - Overflow and type limits
- **Floating point** - Precision, infinity, NaN
- **Type casting** - Implicit/explicit conversions
- **Struct alignment** - Packed vs natural alignment
- **Union type punning** - Type reinterpretation
- **Bitfields** - Bit-level field access
- **Array decay** - Array-to-pointer conversion
- **Volatile qualifier** - Memory-mapped I/O
- **Const correctness** - Immutability patterns
- **Static variables** - Persistent state

**Compile:**
```bash
# Windows (MSVC)
cl /O2 datatype_test.c

# Linux/macOS (GCC) - x86-64
gcc -O2 -o datatype_test datatype_test.c

# macOS (Apple Silicon) - x86-64
gcc -arch x86_64 -O2 -o datatype_test_x64 datatype_test.c

# Cross-compile for Windows (MinGW)
x86_64-w64-mingw32-gcc -O2 -o datatype_test_x64.exe datatype_test.c  # 64-bit
i686-w64-mingw32-gcc -O2 -o datatype_test_x86.exe datatype_test.c    # 32-bit
```

## Testing with Fission

### Basic Usage

```bash
# Show binary info
fission test/control_flow_test.exe --info

# List all functions
fission test/control_flow_test.exe --list

# Decompile specific function
fission test/control_flow_test.exe --decomp 0x140001000

# Decompile all functions
fission test/control_flow_test.exe --decomp-all -o output/

# JSON output
fission test/control_flow_test.exe --decomp 0x140001000 --json
```

### Testing Specific Features

**Control Flow:**
```bash
# Test switch statement recovery
fission test/control_flow_test.exe --decomp classify_number

# Test recursion
fission test/control_flow_test.exe --decomp fibonacci

# Test function pointers
fission test/control_flow_test.exe --decomp calculate
```

**Data Types:**
```bash
# Test struct alignment
fission test/datatype_test.exe --decomp test_struct_alignment

# Test union punning
fission test/datatype_test.exe --decomp test_union_punning

# Test bitfields
fission test/datatype_test.exe --decomp test_bitfields
```

**Windows API:**
```bash
# Test FID matching
fission test/winapi_test.exe --decomp test_file_operations

# With verbose mode to see FID loading
fission test/winapi_test.exe --decomp test_memory_operations -v
```

## Build Script

```bash
#!/bin/bash
# build_tests.sh

echo "Building Fission test suite..."

# Determine architecture
if [[ $(uname -m) == "arm64" ]]; then
    echo "Detected Apple Silicon - compiling for x86-64"
    ARCH_FLAG="-arch x86_64"
else
    ARCH_FLAG=""
fi

# Control flow tests
echo "Building control_flow_test..."
gcc $ARCH_FLAG -O2 -o control_flow_test_x64 control_flow_test.c
x86_64-w64-mingw32-gcc -O2 -o control_flow_test_x64.exe control_flow_test.c
i686-w64-mingw32-gcc -O2 -o control_flow_test_x86.exe control_flow_test.c

# Data type tests
echo "Building datatype_test..."
gcc $ARCH_FLAG -O2 -o datatype_test_x64 datatype_test.c
x86_64-w64-mingw32-gcc -O2 -o datatype_test_x64.exe datatype_test.c
i686-w64-mingw32-gcc -O2 -o datatype_test_x86.exe datatype_test.c

# Structure tests
echo "Building struct_test..."
gcc $ARCH_FLAG -O2 -o struct_test_x64 struct_test.c
x86_64-w64-mingw32-gcc -O2 -o struct_test_x64.exe struct_test.c
i686-w64-mingw32-gcc -O2 -o struct_test_x86.exe struct_test.c

# Windows API tests (Windows only)
echo "Building winapi_test..."
x86_64-w64-mingw32-gcc -O2 -o winapi_test.exe winapi_test.c -ladvapi32 -luser32 -lws2_32

echo "All tests built successfully!"
```

## Expected Decompilation Quality

### Excellent (95-100% accuracy):
- Simple arithmetic functions (add, subtract, multiply, divide)
- Basic control flow (if/else, simple loops)
- Structure field access
- String references and constants
- Windows API calls with FID

**Test Results (Windows PE x86-64):**
- ✅ `add()` - Perfectly recovered: `return param_1 + param_2;`
- ✅ `classify_number()` - String references preserved with addresses
- ✅ Simple conditionals and comparisons

### Good (85-95% accuracy):
- Switch statements (may optimize to jump tables)
- Nested loops and recursion
- Function pointers
- Type inference
- Pointer arithmetic

**Test Results (Windows PE x86-64):**
- ✅ `fibonacci()` - Recursive structure preserved with loop optimization
- ✅ `factorial()` - Tail recursion converted to loop
- ✅ `reverse_array()` - Pointer manipulation correctly decompiled

### Moderate (70-85% accuracy):
- Inline assembly
- Variadic functions
- Union type punning
- Optimized tail calls
- Complex pointer arithmetic

**Test Results (Windows PE x86-64):**
- ⚠️ printf calls - Shows function pointer calls but not formatted strings
- ⚠️ Complex control flow with multiple optimizations

### Challenging (<70% accuracy):
- Heavy optimization (inlining, unrolling)
- SIMD intrinsics
- Template instantiation artifacts
- Exception handling (C++)
- Mach-O binaries (limited support, PE is preferred)

## Validation Checklist

### For struct_test.exe:
- [ ] `Item` structure definition appears in decompiled code
- [ ] Field accesses show correct offsets and types
- [ ] `point.x` and `point.y` nested fields are detected
- [ ] Function parameter is typed as `Item*`

### For winapi_test.exe:
- [ ] Import functions have correct names (not generic sub_*)
- [ ] Constants are resolved (GENERIC_READ, MEM_COMMIT, etc.)
- [ ] Constants are resolved (GENERIC_READ, MEM_COMMIT, etc.)
- [ ] Flag combinations are detected (e.g., GENERIC_READ | GENERIC_WRITE)
- [ ] Function signatures match Windows API
- [ ] DLL names are correct (kernel32.dll, advapi32.dll, etc.)

### For control_flow_test.exe ✨ NEW:
- [ ] Switch statement recovers as switch (not if-else chain)
- [ ] Loop structures properly detected
- [ ] Recursion shows function calls (not tail-call jumps)
- [ ] Function pointer calls identified
- [ ] Inline assembly blocks preserved

### For datatype_test.exe ✨ NEW:
- [ ] Integer types correctly inferred
- [ ] Struct padding/alignment detected
- [ ] Union members identified
- [ ] Bitfield access patterns recognized
- [ ] Const/volatile qualifiers preserved

## Contributing

When adding new test cases:

1. **Document the purpose** - What decompilation feature does it test?
2. **Keep it focused** - One test per specific feature
3. **Add expected output** - What should the decompiler produce?
4. **Test across compilers** - MSVC, GCC, Clang
5. **Vary optimization levels** - `/O1`, `/O2`, `/Ox`

## Known Issues

- **Tail call optimization**: May appear as jumps instead of calls
- **Loop unrolling**: Duplicated code instead of loop structures  
- **Inline assembly**: Platform-specific, may not decompile cleanly
- **Floating point**: Precision issues with constant folding

## References

- Ghidra Decompiler: https://ghidra-sre.org/
- IDA Hex-Rays: https://hex-rays.com/
- FID Database Format: See `ghidra/funtionID/building_fid.txt`
- Use `-O0` to prevent aggressive optimization that might obscure patterns
- For FID testing, ensure the appropriate `.fidbf` file is loaded (VS2019 recommended)
- Some API calls may fail at runtime (by design) but should still be recognizable in static analysis
