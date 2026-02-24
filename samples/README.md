# Samples

Real-world / commercial binaries for decompiler quality testing.

Binary files are **gitignored** — only directory structure, `.gitkeep`,
and suite YAML files are tracked.

## Structure

```
samples/
  windows/
    x64/    ← stripped PE32+ (MSVC, MinGW, Clang-CL, ...)
    x86/    ← stripped PE32 (MSVC, MinGW, ...)
  linux/
    x64/    ← ELF x86-64 (GCC, Clang, ...)
    x86/    ← ELF i386
  macos/
    arm64/  ← Mach-O ARM64 (Apple Silicon)
    x64/    ← Mach-O x86-64
```

## Adding a new sample

1. Drop the binary into the appropriate subdirectory.
2. Create a suite YAML beside it:
   ```
   samples/windows/x64/my_app_suite.yaml
   ```
3. Run the benchmark:
   ```bash
   python3 scripts/compare/compare_decompilers_v3.py \
       --suite samples/windows/x64/my_app_suite.yaml \
       scripts/result
   ```

Suite YAML files **are** tracked — commit them to record which
functions / addresses were tested and what expected patterns apply.
