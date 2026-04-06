# RetDec Backend Integration Plan

> ⚠️ **Status: Historical / Experimental Plan**
> This document captures an earlier experimental idea for integrating RetDec into Fission.
> It is **not** on the current active roadmap.
> For the current direction, prefer [`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md) and [`docs/ROADMAP.md`](../ROADMAP.md).

## Goal

This plan explored whether integrating RetDec's `llvmir2hll` could improve decompilation readability beyond the existing Ghidra-backed path.

At the time, the rough target was to raise similarity against Ghidra from around `30%` to `60%+` on selected cases.

## Architecture Sketch

### Existing Pipeline At The Time

```
Binary -> Ghidra P-code lifter -> CFG -> Type propagation -> C code emitter
```

### Experimental Hybrid Pipeline

```
Binary -> Ghidra P-code lifter -> CFG -> [RetDec-enhanced CFG optimizer] -> C code emitter
                                      ↓
                               [Optional: LLVM IR export]
                                      ↓
                               RetDec llvmir2hll
```

## Original Implementation Phases

### Phase 1: Build RetDec Libraries

- download and build RetDec dependencies
- verify `libretdec-llvmir2hll.a` / `.dylib`
- add RetDec linking to the Fission CMake build

### Phase 2: P-code → LLVM IR Conversion

- add `legacy-native-decompiler-tree/src/ir/PcodeToLLVM.cc`
- map Ghidra P-code ops into LLVM IR instructions
- provide a per-function LLVM module construction API

### Phase 3: RetDec HLL Writer Integration

- add `RetDecBackend.h/.cc`
- call `LlvmIr2Hll::runOnModule()`
- extract generated C text
- merge the result with existing post-processing

### Phase 4: Backend Selection and Fallback

- CLI flag: `--backend=ghidra|retdec`
- GUI backend selector
- fallback to standard Ghidra output when RetDec fails

### Phase 5: Test and Benchmark

- rerun the complex test suite
- compare similarity before/after
- record results in the changelog

## Technical Notes

### Example P-code → LLVM IR Mapping

| P-code Op | LLVM IR |
|-----------|---------|
| `COPY` | direct assignment / load-store pattern |
| `INT_ADD` | `add` |
| `INT_SUB` | `sub` |
| `INT_MULT` | `mul` |
| `INT_DIV` | `sdiv` |
| `INT_AND` | `and` |
| `INT_OR` | `or` |
| `INT_XOR` | `xor` |
| `LOAD` | `load` |
| `STORE` | `store` |
| `BRANCH` | `br` |
| `CBRANCH` | conditional `br` |
| `CALL` | `call` |
| `RETURN` | `ret` |

### Example RetDec API Usage

```cpp
#include "retdec/llvmir2hll/llvmir2hll.h"
#include "retdec/config/config.h"

std::string decompileWithRetDec(llvm::Module& module) {
    std::string output;
    retdec::config::Config config;

    retdec::llvmir2hll::LlvmIr2Hll decompiler(&config);
    decompiler.setOutputString(&output);
    decompiler.runOnModule(module);

    return output;
}
```

## Risks Identified At The Time

| Risk | Mitigation |
|------|------------|
| LLVM version mismatch | build against the LLVM version RetDec expects |
| unsupported P-code operations | fall back to Ghidra output |
| long RetDec build times | cache prebuilt binaries or handle in CI |
| increased memory usage | release LLVM modules quickly and process per function |

## Success Criteria Considered

- average complex-test similarity at or above `50%`
- single-function decompilation stays within ~5 seconds
- backend selection exposed in the UI
- automatic fallback works when RetDec fails

## References

- RetDec GitHub: <https://github.com/avast/retdec>
- LLVM IR reference: <https://llvm.org/docs/LangRef.html>
- Ghidra P-code reference: `legacy-native-decompiler-tree/decompile/cpp/opbehavior.hh`
