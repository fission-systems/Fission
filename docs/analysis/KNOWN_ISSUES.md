# Known Issues (Analysis)

## ~~TypePropagator STORE/CALL High-Type Logging~~ [RESOLVED]

### Status: **RESOLVED** (2026-01-08)

### Original Symptoms
- `compare_decompilers_v2.py` runs complete, but no `[TypePropagator] STORE/CALL` logs appear in the
  generated `*_run.log` when decompiling `0x140001680` from `test/comparison_test_x64.exe`.
- After the recent rebuild, `fission_cli --decomp` can fail with:
  `Unknown decompilation error` (seen in `scripts/result/202601081137_result/comparison_fission_decomp.txt`).

### Root Causes Found

1. **LowlevelError: "Requesting non-existent high-level"**
   - In `TypePropagator::propagate_struct_types()`, the `value_is_pointer` lambda called
     `vn->getHighTypeDefFacing()` without proper exception handling.
   - When `vn->getHigh()` returns non-null but the high variable isn't fully initialized,
     accessing high type methods throws a `LowlevelError`.
   - **Fix**: Added try-catch around high variable access in the lambda.

2. **LowlevelError: "Can only set fields on an incomplete structure"**
   - In `TypePropagator::propagate_struct_types()`, the code attempted to call
     `tf->setFields()` on already-complete (finalized) structures.
   - Ghidra's TypeFactory only allows field modification on incomplete structures.
   - **Fix**: Added try-catch around `setFields()` to gracefully handle this case.

### Files Modified
- `ghidra_decompiler/src/analysis/TypePropagator.cc`
  - Added exception handling for high variable access
  - Added exception handling for structure field modification

### Verification
```bash
# Run comparison benchmark
python3 scripts/compare/compare_decompilers_v2.py test/comparison_test_x64.exe 0x140001680

# Expected result:
# - Fission: 33 lines, 1 branches
# - Ghidra: 37 lines, 1 branches
# - Similarity: ~23%
```

### Notes
- The "Unknown decompilation error" was a generic catch-all error from the FFI layer
- The actual errors were Ghidra `LowlevelError` exceptions that weren't being caught properly
- TypePropagator STORE/CALL logs now appear when running with `--verbose` flag

---

## Server Mode SIGSEGV (Pending Investigation)

### Status: **OPEN**

### Symptoms
- Server mode (`fission_decomp --server`) crashes with SIGSEGV when processing
  `load_bin` followed by `decompile` commands.

### Reproduction
```bash
ghidra_decompiler/build/fission_decomp --server < scripts/result/fission_decomp_server_input.txt
```

### Impact
- Cannot use server mode for persistent decompilation sessions
- Single-shot mode works correctly after the TypePropagator fixes

### Next Steps
1. Validate JSON parsing for `load_bin` and `decompile` commands
2. Audit memory lifecycle for `DecompilerContext` between requests
3. Check for use-after-free or double-free issues
