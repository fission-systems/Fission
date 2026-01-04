# FID (Function ID) Database Support

Fission uses Function ID databases to automatically identify and name library functions in decompiled code. This dramatically improves decompilation quality by showing actual function names instead of generic `FUN_0x...` labels.

## Supported FID Databases

### Currently Included:
- **Visual Studio 2019** (x86/x64) - MSVC runtime
- **Visual Studio 2017** (x86/x64) - MSVC runtime  
- **Visual Studio 2015** (x86/x64) - MSVC runtime
- **Visual Studio 2012** (x86/x64) - MSVC runtime
- **Visual Studio Older** (x86/x64) - VS 2003-2010

### 🆕 GCC/MinGW Support (NEW!)
Fission now supports automatic function identification for GCC and MinGW compiled binaries!

**To enable GCC/MinGW FID:**
1. Follow the guide in [`ghidra/funtionID/BUILDING_GCC_FID.md`](../ghidra/funtionID/BUILDING_GCC_FID.md)
2. Generate FID databases using Ghidra
3. Place `.fidbf` files in `ghidra/funtionID/`

**Expected file names:**
- `gcc13_x64.fidbf` - GCC 13 x86-64 runtime
- `gcc13_x86.fidbf` - GCC 13 x86 runtime
- `mingw_x64.fidbf` - MinGW x86-64 runtime
- `mingw_x86.fidbf` - MinGW x86 runtime

## Quick Start

### Building FID Sample Programs

```bash
cd ghidra/funtionID
./build_fid_samples.sh
```

This creates test binaries in `fid_test_binaries/` compiled with various GCC/MinGW configurations.

### Quality Improvement

**Before FID:**
```c
void FUN_0x140001000(void) {
  FUN_0x140001050(param_1, 100);
  FUN_0x140001100(param_2);
}
```

**After FID:**
```c
void process_data(void) {
  malloc(param_1, 100);
  printf(param_2);
}
```

## FID Database Priority

Fission loads FID databases in this order:
1. GCC 13, 12, 11 (newest to oldest)
2. MinGW (generic)
3. Visual Studio 2019, 2017, 2015, 2012, Older

This ensures optimal matching for both GCC-compiled and MSVC-compiled binaries.

## Common Symbols

Common symbols files filter out overly generic functions that cause false positives:

- **`common_symbols_win64.txt`** - Windows x64 MSVC
- **`common_symbols_win32.txt`** - Windows x86 MSVC
- **`common_symbols_gcc_x64.txt`** ✨ NEW - GCC/MinGW x64
- **`common_symbols_gcc_x86.txt`** ✨ NEW - GCC/MinGW x86

## Creating Custom FID Databases

See [`BUILDING_GCC_FID.md`](../ghidra/funtionID/BUILDING_GCC_FID.md) for detailed instructions on:
- Setting up Ghidra for FID generation
- Compiling sample libraries
- Populating and refining databases
- Exporting to `.fidbf` format
- Testing with Fission

## Troubleshooting

### Functions Not Recognized

If library functions aren't being identified:

1. **Check FID files exist:**
   ```bash
   ls -lh ghidra/funtionID/*.fidbf
   ```

2. **Verify correct architecture:**
   - x86 binaries need `*_x86.fidbf`
   - x64 binaries need `*_x64.fidbf`

3. **Check verbose output:**
   ```bash
   cargo run --bin fission_cli --features native_decomp -- test.exe --decomp 0x140001000 --verbose
   ```
   Look for `[✓] FID database loaded` messages

4. **Generate missing databases:**
   Follow the guide in `BUILDING_GCC_FID.md`

### False Positives

If functions are misidentified:

1. Update common symbols file with problematic functions
2. Regenerate FID database
3. Or manually mark functions as auto-fail in Ghidra

## Performance Impact

- **Loading time:** +100-300ms per FID database
- **Memory usage:** +5-20MB per database
- **Matching speed:** Negligible (<1% overhead)

## Contributing FID Databases

Have high-quality FID databases to share?

1. Ensure they're properly filtered (no false positives)
2. Document compilation settings
3. Test with diverse binaries
4. Submit PR with:
   - `.fidbf` file
   - Common symbols file
   - Build instructions

## References

- **Ghidra FID Format:** [NSA Ghidra Documentation](https://ghidra-sre.org/)
- **Fission FID Loader:** `ghidra_decompiler/src/analysis/FidDatabase.cc`
- **FID Matcher:** `ghidra_decompiler/src/analysis/FunctionMatcher.cc`

---

**Last Updated:** 2026-01-04  
**Fission Version:** 0.1.0+gcc-fid
