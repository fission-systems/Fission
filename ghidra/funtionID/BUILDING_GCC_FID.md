# Building GCC/MinGW FID Databases for Fission

This guide explains how to create Function ID (FID) databases for GCC and MinGW compiled binaries to improve Fission's decompilation quality.

## Overview

FID databases allow Fission to automatically identify library functions by matching their binary patterns (hashes). This dramatically improves decompilation output by showing actual function names instead of generic `FUN_0x...` labels.

## Prerequisites

1. **Ghidra** (version 10.0 or later) - https://ghidra-sre.org/
2. **Sample binaries** compiled with GCC/MinGW at various optimization levels
3. **Common symbols file** - `common_symbols_gcc_x64.txt` or `common_symbols_gcc_x86.txt`

## Step-by-Step Guide

### 1. Collect Sample Binaries

You need representative binaries compiled with different GCC versions and settings:

```bash
# Create a directory structure
mkdir -p fid_sources/gcc-9/x64/{debug,release}
mkdir -p fid_sources/gcc-10/x64/{debug,release}
mkdir -p fid_sources/gcc-11/x64/{debug,release}
mkdir -p fid_sources/gcc-12/x64/{debug,release}
mkdir -p fid_sources/gcc-13/x64/{debug,release}

# Similar structure for x86
mkdir -p fid_sources/mingw-9/x86/{debug,release}
mkdir -p fid_sources/mingw-10/x86/{debug,release}
```

#### Sample Programs to Compile:

Create a simple test program that uses common functions:

**test_runtime.c:**
```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

int main() {
    // Memory allocation
    char *buffer = (char*)malloc(100);
    strcpy(buffer, "Hello, World!");
    printf("%s\n", buffer);
    
    // Math operations
    double result = sqrt(16.0);
    printf("sqrt(16) = %f\n", result);
    
    // String operations
    char *pos = strchr(buffer, 'W');
    if (pos) {
        printf("Found at position: %ld\n", pos - buffer);
    }
    
    free(buffer);
    return 0;
}
```

**Compile for different configurations:**

```bash
# GCC 13 x64 Debug
gcc-13 -m64 -g -O0 -o fid_sources/gcc-13/x64/debug/test_runtime.exe test_runtime.c

# GCC 13 x64 Release
gcc-13 -m64 -O2 -o fid_sources/gcc-13/x64/release/test_runtime.exe test_runtime.c

# MinGW x86 Debug
i686-w64-mingw32-gcc -g -O0 -o fid_sources/mingw/x86/debug/test_runtime.exe test_runtime.c

# MinGW x86 Release
i686-w64-mingw32-gcc -O2 -o fid_sources/mingw/x86/release/test_runtime.exe test_runtime.c
```

### 2. Open Ghidra and Create FID Database

1. Launch Ghidra
2. Create or open a project
3. Go to **Tools → Function ID → Create FidDb...**
4. Configure the database:
   - **Name:** `gcc13_x64` (or appropriate name)
   - **Target Architecture:** x86:LE:64:default (for x64) or x86:LE:32:default (for x86)
   - Click **Create**

### 3. Populate FID Database

1. Go to **Tools → Function ID → Populate FidDb from programs...**
2. Select your newly created FID database
3. **Important:** Click "Common Symbols File" and select:
   - `common_symbols_gcc_x64.txt` for x64 builds
   - `common_symbols_gcc_x86.txt` for x86 builds
4. Add all sample binaries from your `fid_sources` directory
5. Click **OK** and wait for analysis to complete

### 4. Review and Refine

After population, Ghidra will show statistics:
- Number of functions added
- Most common symbols encountered

**Recommended refinements:**

Run Ghidra scripts to clean up the database:

```java
// RemoveFunctions.java - Example cleanup script
// This sets auto-fail for overly generic functions

import ghidra.program.model.listing.*;
import ghidra.feature.fid.service.*;

FidService fidService = new FidService();
FidDB fidDb = fidService.openFidDB("/path/to/gcc13_x64.fidb", false);

// Auto-fail for tiny generic functions (< 10 bytes)
// These often cause false positives
List<FidFunction> functions = fidDb.getAllFunctions();
for (FidFunction func : functions) {
    if (func.getCodeUnitSize() < 10) {
        func.setAutoFail(true);
    }
}

fidDb.save();
fidDb.close();
```

### 5. Export FID Database

1. Go to **Tools → Function ID → Export FidDb...**
2. Select your database
3. Export to `.fidb` format
4. Rename to follow Fission's naming convention:
   - `gcc13_x64.fidbf` for GCC 13 x64
   - `mingw_x64.fidbf` for MinGW x64
   - `gcc13_x86.fidbf` for GCC 13 x86
   - `mingw_x86.fidbf` for MinGW x86

### 6. Install in Fission

Copy the `.fidbf` files to:
```
Fission/ghidra/funtionID/
```

Files should be named:
- `gcc13_x64.fidbf`
- `gcc13_x86.fidbf`
- `mingw_x64.fidbf`
- `mingw_x86.fidbf`

## Automated Build Script (Optional)

For CI/CD, you can automate FID generation using Ghidra's headless analyzer:

```bash
#!/bin/bash
# build_gcc_fid.sh

GHIDRA_HOME="/path/to/ghidra"
PROJECT_DIR="./fid_project"
FID_NAME="gcc13_x64"

# Create project
$GHIDRA_HOME/support/analyzeHeadless $PROJECT_DIR TempProject -import fid_sources/gcc-13/x64/debug/*.exe -recursive

# Populate FID (requires custom Ghidra script)
$GHIDRA_HOME/support/analyzeHeadless $PROJECT_DIR TempProject \
    -postScript PopulateFidDb.java $FID_NAME common_symbols_gcc_x64.txt

# Export
$GHIDRA_HOME/support/analyzeHeadless $PROJECT_DIR TempProject \
    -postScript ExportFidDb.java $FID_NAME gcc13_x64.fidbf

echo "FID database created: gcc13_x64.fidbf"
```

## Testing the FID Database

After installing, test with Fission:

```bash
# Compile a test program with GCC
gcc -O2 -o test.exe test_program.c

# Decompile with Fission
cargo run --bin fission_cli --features native_decomp -- test.exe --list

# You should see actual function names instead of FUN_0x... labels
```

**Expected output improvement:**

Before FID:
```
0x140001000  FUN_0x140001000
0x140001050  FUN_0x140001050
0x140001100  FUN_0x140001100
```

After FID:
```
0x140001000  malloc
0x140001050  strcpy
0x140001100  printf
```

## Recommended Library Collections

For comprehensive coverage, create FID databases for:

### Essential Libraries:
- **libc** - C standard library (malloc, printf, strcpy, etc.)
- **libm** - Math library (sin, cos, sqrt, pow, etc.)
- **libstdc++** - C++ standard library
- **libgcc** - GCC runtime support

### Common Third-Party Libraries:
- **OpenSSL** (libssl, libcrypto)
- **zlib** - Compression
- **libpng** - PNG image handling
- **libcurl** - HTTP client
- **SQLite** - Database engine

### Windows-Specific (MinGW):
- **MinGW CRT** - C runtime
- **ucrt** - Universal C Runtime
- **kernel32 wrappers**

## Maintenance

FID databases should be updated when:
1. New GCC versions are released
2. Common library versions change
3. You encounter binaries with unrecognized functions

## Troubleshooting

### Database Size Too Large

If your `.fidbf` file is too large (>50MB):
1. Use stricter common symbols filtering
2. Remove debug builds (keep only release)
3. Run defragmentation with RepackFid.java

### Too Many False Positives

If you get incorrect function matches:
1. Review and expand the common symbols file
2. Set auto-fail for problematic functions
3. Increase minimum function size threshold

### Missing Symbols

If important functions aren't recognized:
1. Verify they exist in your sample binaries
2. Check if they're being filtered as common symbols
3. Add more diverse sample binaries

## Additional Resources

- **Ghidra FID Documentation:** [ghidra-sre.org](https://ghidra.re)
- **FID Format Specification:** See Ghidra source code
- **Fission FID Loader:** `ghidra_decompiler/src/analysis/FidDatabase.cc`

## Contributing

If you create high-quality FID databases for GCC/MinGW, consider:
1. Sharing them with the Fission community
2. Documenting your compilation parameters
3. Including version information in the filename

Example naming: `gcc13.2_x64_O2_v1.fidbf`

---

**Last Updated:** 2026-01-04
**Fission Version:** 0.1.0
**Ghidra Version:** 10.x+
