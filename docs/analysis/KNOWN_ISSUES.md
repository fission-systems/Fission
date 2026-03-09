# Known Issues (Analysis)

> ℹ️ **Note:** 이 문서는 **현재 기준으로 열려 있는 분석 관련 Known Issue**를 추적하기 위한 용도입니다.  
> 이미 해결된 이슈는 요약만 남기고, 상세 내용과 변경 사항은 `docs/changelog/CHANGELOG.md`를 참고하세요.

---

## Duplicate VariablePiece (Ghidra Merge 충돌) [MITIGATED]

### Status: **MITIGATED** (2026-03)

### 증상
- 일부 함수(예: putty.exe `0x140001080`) 디컴파일 시 `Ghidra LowlevelError: Duplicate VariablePiece` 발생
- 에러 메시지: `retry without seed failed` (재시도 후에도 실패하는 경우)

### 발생 원인
- **PrototypeEnforcer**가 주입한 엄격한 API 시그니처(HWND, LPCSTR 등 8바이트 포인터)와 Ghidra의 **Merge** 로직이 충돌
- `variable.cc` `VariableGroup::addPiece()`: 동일한 (offset, size)의 VariablePiece를 그룹에 중복 등록 시도 시 예외 발생
- 호출자(Caller) P-code가 인자를 여러 레지스터 조각(Piece)으로 전달하거나, 타입 크기 불일치가 있을 때 충돌
- IDA/Ghidra 원본에서도 흔한 **정적 분석의 근본적 한계** 중 하나

### 현재 방어 로직 (Upstream 코드 수정 없이)
1. **FFI 레벨**: `ghidra::LowlevelError` 전용 catch 추가 (`std::exception` 미상속으로 기존 `catch(...)`에서 누락되던 실제 메시지 노출)
2. **DecompilationCore 재시도**: Duplicate VariablePiece 발생 시 `seed_before_action` 없이 재시도 (1회)
3. **run_analysis_passes**: 해당 단계에서 발생 시 빈 `AnalysisArtifacts`로 계속 진행하여 기본 디컴파일 결과 유지

### 수정된 파일
- `ghidra_decompiler/src/decompiler/DecompilationCore.cpp` — 재시도 로직, `run_analysis_passes` 예외 처리
- `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp` — `LowlevelError` catch 추가

### 향후 개선 아이디어
- **Assembly Fallback**: Duplicate VariablePiece 발생 시 해당 함수는 디컴파일을 건너뛰고, 디스어셈블리(Assembly)로 폴백하여 사용자에게 보여주는 기능 검토

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
