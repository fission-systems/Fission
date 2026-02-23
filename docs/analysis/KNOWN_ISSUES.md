# Known Issues (Analysis)

> ℹ️ **Note:** 이 문서는 **현재 기준으로 열려 있는 분석 관련 Known Issue**를 추적하기 위한 용도입니다.  
> 이미 해결된 이슈는 요약만 남기고, 상세 내용과 변경 사항은 `docs/changelog/CHANGELOG.md`를 참고하세요.

## ~~TypePropagator STORE/CALL High-Type Logging~~ [RESOLVED]

### Status: **RESOLVED** (2026-01-08, see also: `docs/changelog/CHANGELOG.md`)

### Original Symptoms
- `compare_decompilers_v2.py` runs complete, but no `[TypePropagator] STORE/CALL` logs appear in the
  generated `*_run.log` when decompiling `0x140001680` from `examples/comparison_test_x64.exe`.
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
python3 scripts/compare/compare_decompilers_v2.py examples/comparison_test_x64.exe 0x140001680

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

## ~~Server Mode SIGSEGV~~ [OBSOLETE]

### Status: **REMOVED** (see also: `docs/changelog/CHANGELOG.md`)

Server mode (`fission_decomp --server`) has been removed. The decompiler now operates
in single-shot mode only (`main.cc` inlines the request pipeline directly).
`ServerMode.cc` / `ServerMode.h` have been deleted.
