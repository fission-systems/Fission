# Windows ARM64 Spike

## Current Status

- PE loader now recognizes `IMAGE_FILE_MACHINE_ARM64` as `AARCH64:LE:64:v8A`.
- CLI binary info surfaces ARM64 correctly in JSON/terminal output when `arch_spec` is `AARCH64`.
- Preview/runtime bring-up is **not** complete yet.

## Current Blocker

- There is no real Windows ARM64 sample binary in [`/Users/sjkim1127/Fission/samples`](/Users/sjkim1127/Fission/samples) to build a fixed-seed baseline from.
- Existing ARM64 samples are macOS Mach-O only, and `ida76sp1` `hexarm64.dll` / `procs/arm64.dll` are x86-64 host DLLs, not ARM64 PE targets.

## Next Bring-Up Checks

1. Add 2-3 real Windows ARM64 PE samples.
2. Record `binary-info --json` output and confirm `arch = arm64`.
3. Run fixed-seed decompile on 3-5 functions per sample.
4. Capture direct preview / native fallback / assembly fallback distribution.
5. Identify the first hard gap among:
   - register naming
   - branch condition lowering
   - stack/local slot surfacing
   - type/context plumbing

## Success Gate For Promotion

- No hard crash or hang on Windows ARM64 samples
- Result terminates as direct preview or explicit fallback taxonomy
- x86/x64 regression remains clean
