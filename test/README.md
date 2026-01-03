# Fission Test Cases

This directory contains test binaries for validating Fission's analysis capabilities.

## Test Files

### struct_test.c / struct_test.exe
**Purpose**: Test structure recovery and type inference

**Features tested**:
- Structure field detection
- Nested structure handling
- Pointer dereferencing
- Type propagation through function calls

**Expected results**:
- Fission should recover the `Item` structure definition
- Field accesses should be properly typed
- Nested structure (`point`) should be detected

### winapi_test.c / winapi_test.exe
**Purpose**: Test FID (Function ID) matching and constant substitution

**Features tested**:
- Automatic function name recognition via FID database
- Windows API constant resolution (GENERIC_READ, MEM_COMMIT, etc.)
- Multiple DLL coverage (kernel32, advapi32, ws2_32, bcrypt)
- Flag combination detection (e.g., `0x3000` → `MEM_COMMIT | MEM_RESERVE`)

**Expected results**:
- Import functions should be named (not `FUN_140001234`)
- Constants should be substituted with symbolic names
- Function signatures should match Windows API prototypes

**Covered APIs**:
- **File I/O**: CreateFileA, WriteFile, CloseHandle
- **Memory**: VirtualAlloc, VirtualFree, HeapAlloc, HeapFree
- **Registry**: RegOpenKeyExA, RegQueryValueExA, RegCloseKey
- **Network**: WSAStartup, socket, closesocket, WSACleanup
- **Process**: GetCurrentProcess, CreateThread, ResumeThread, WaitForSingleObject
- **Crypto**: BCryptOpenAlgorithmProvider, BCryptCloseAlgorithmProvider

## Building Tests

### Windows (Visual Studio)
```cmd
cl /Od /Zi struct_test.c
cl /Od /Zi winapi_test.c ws2_32.lib advapi32.lib bcrypt.lib
```

### Windows (MinGW)
```bash
gcc -O0 -g struct_test.c -o struct_test.exe
gcc -O0 -g winapi_test.c -o winapi_test.exe -lws2_32 -ladvapi32 -lbcrypt
```

### Cross-compile from Linux (mingw-w64)
```bash
x86_64-w64-mingw32-gcc -O0 -g struct_test.c -o struct_test.exe
x86_64-w64-mingw32-gcc -O0 -g winapi_test.c -o winapi_test.exe -lws2_32 -ladvapi32 -lbcrypt
```

## Running Tests

### Direct Analysis
```bash
# Structure recovery test
fission --cli test/struct_test.exe --info
fission --cli test/struct_test.exe --sections

# FID matching test
fission --cli test/winapi_test.exe --info
fission --cli test/winapi_test.exe --count

# Decompile specific function
fission --cli test/winapi_test.exe
fission> funcs
fission> decompile 0x140001000  # Replace with actual address
```

### REPL Mode
```bash
fission --cli test/winapi_test.exe
fission> info
fission> sections
fission> funcs
fission> decompile <address>
```

## Validation Checklist

### For struct_test.exe:
- [ ] `Item` structure definition appears in decompiled code
- [ ] Field accesses show correct offsets and types
- [ ] `point.x` and `point.y` nested fields are detected
- [ ] Function parameter is typed as `Item*`

### For winapi_test.exe:
- [ ] Import functions have correct names (not generic sub_*)
- [ ] Constants are resolved (GENERIC_READ, MEM_COMMIT, etc.)
- [ ] Flag combinations are detected (e.g., GENERIC_READ | GENERIC_WRITE)
- [ ] Function signatures match Windows API
- [ ] DLL names are correct (kernel32.dll, advapi32.dll, etc.)
- [ ] Cross-references work for API calls

## Notes

- Test binaries should be compiled with symbols (`/Zi` or `-g`) for easier validation
- Use `-O0` to prevent aggressive optimization that might obscure patterns
- For FID testing, ensure the appropriate `.fidbf` file is loaded (VS2019 recommended)
- Some API calls may fail at runtime (by design) but should still be recognizable in static analysis
