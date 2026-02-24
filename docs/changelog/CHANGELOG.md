# Changelog

All notable changes to the Fission project (November 2025 – Present).

---

## [Unreleased / HEAD] — 2026-02-25

### x86 Double Argument Synthesis + Benchmark Normalization Fixes

**Decompiler Quality: x86 90.1% → 92.6% | x64 98.8% (maintained)**

#### x86 `double` Argument Synthesis (Track 1)

x86 cdecl passes a `double` (8 B) as two consecutive 4-byte stack pushes. Ghidra's Pcode
`CPUI_CALL` therefore had two 4-byte input varnodes where the callee prototype expected one
8-byte parameter. This caused Fission to emit `create_item(..., 0x51eb851f, 0x4048feb8)`
instead of `create_item(..., 0x4048feb851eb851f)`.

**Three bugs fixed in `merge_split_double_args`:**

1. **`isConstant()` guard removed** — CPUI_CALL target lives in code/fspec space, never CONST.
2. **`queryFunction(defaultCodeSpace, addr)` → `fd->getCallSpecs(call_op)`** — x86 externals
   live in fspec address space; `queryFunction` always returned null.
3. **`getProto()` removed** — `FuncCallSpecs` IS-A `FuncProto`; use `fc->numParams()` /
   `fc->getParam()` directly.

**Result:** `_create_item(0x3e9,"TestItem",0x51eb851f,0x4048feb8)`
→ `_create_item(0x3e9,"TestItem",0x4048feb851eb851f)` ✅

#### Benchmark Normalization Fixes (`compare_decompilers_v3.py`)

- **VAR digit suffix bug**: `p[vucslt]Var(\d+) → VAR\1` kept digit; fixed to strip it.
- **Missing xVar pattern**: `xVar2`, `mVar1` etc. not normalised; added `[a-z]Var[0-9]+ → VAR`.

**Score impact (main function):** 40.0% → 60.0%

#### Files Changed

| File | Change |
|------|--------|
| `ghidra_decompiler/src/analysis/TypePropagator.cc` | `merge_split_double_args` rewrite |
| `ghidra_decompiler/include/fission/analysis/TypePropagator.h` | method moved to public |
| `ghidra_decompiler/src/decompiler/AnalysisPipeline.cpp` | post-barrier call (FFI + batch) |
| `scripts/compare/compare_decompilers_v3.py` | VAR/xVar normalization bug fixes |

---

## 2026-02-24

### Track 2 / 3 / 4 + Benchmark Normalization (x86 80.0% → 90.1%)

**Decompiler Quality: x86 80.0% → 90.1% (+10.1 pp)**

#### Track 2 — Pointer Return Type Inference

`create_item` returns a `void*` heap pointer. Fission previously inferred the return type as a
scalar int. Added allocator-like heuristic in `TypePropagator::propagate_call_return_types`.

#### Track 3 — Array Argument Semantics

Improved `propagate_from_call` handling for pointer parameters bound to local arrays.

#### Track 4 — CLI Header Strip

`--no-header` flag added; suppresses `// Function: NAME @ 0xADDR` banner from `--decomp` output.

#### Normalizer A-1 through A-6

| Rule | Description |
|------|-------------|
| A-1 | Remove `!= (Type*)0x0` null-pointer comparisons |
| A-2 | `char*`/`byte*`/`uchar*` → `OPAQUE_PTR` |
| A-3 | `f_<hex>*` inferred-struct pointers → `OPAQUE_PTR` |
| A-4 | `UNDEF*` → `OPAQUE_PTR` |
| A-5 | `uint*/ushort*` → `UNDEF` |
| A-6 | Remove `(UNDEF)` cast expressions |

---

## 2026-02-23

### Tauri GUI — Phase 1–9 완전 이관 + egui 제거

Egui 기반 `fission-ui`를 **Tauri 2.x + React 19 / TypeScript**로 완전 이관.

| Phase | 핵심 내용 |
|-------|----------|
| 1 | StringXrefs 클릭 내비게이션, 어셈블리 복사 버튼, Cmd+←/→ 탭 순환, 정규식 필터, CFG V(G) |
| 2 | `@tanstack/react-virtual` 가상 스크롤 (5,000+ 라인) |
| 3 | FID 시그니처 식별 (`run_fid` Tauri 커맨드) |
| 4 | 디버그 메모리 덤프 (최대 4 KB) |
| 5 | TTD 타임라인 (5개 커맨드, Windows SingleStep) |
| 6 | UTF-16 LE 문자열 스캔 + StringXrefs 가상 스크롤 |
| 7 | CFG 팬/줌 + UI 스케일 슬라이더 (50%–200%) |
| 8 | 분석 결과 JSON 내보내기 |
| 9 | `crates/fission-ui/` egui 코드 완전 제거 (~6,000 LOC) |

---

## 2026-02-21

### Tauri GUI — Analyze Functions / Deep Scan (Phase 6)

- `analyze_functions`: CALL 타깃 스캔으로 내부 함수 발굴
- `deep_scan_functions`: 프롤로그 패턴 스캔
- FunctionsList: 카테고리 필터, Analyze 🔍 / Deep Scan 🕵 툴바 버튼
- MenuBar: Tools 메뉴 (F5/F6 단축키)

---

## 2026-02-20

### Windows/MSVC Build Compatibility + Tauri GUI

**MSVC 빌드 호환성 5건:** dlfcn.h → GetProcAddress, cxxabi.h → Dbghelp.h/UnDecorateSymbolName,
`__builtin_bswap32` → `_byteswap_ulong`, `std::regex::multiline` 제거, vcpkg auto-link.

**Tauri GUI Phase 1–5:** 30+ IPC 커맨드, 프로젝트 저장/로드 (`.fprj`), Listing 가상 스크롤,
CFG SVG 렌더링, 디버거 UI (Windows).

---

## 2026-02-17

### Analysis Pipeline & Data Section Scan 공통화

- `BatchAnalysisContext` 어댑터: FFI/배치 경로 통합 (`run_analysis_passes()`)
- `scanAndRegisterDataSymbols()` 통합 API: PE 파싱 + 심볼 등록 일원화
- FFI: `decomp_set_function_*`, `decomp_set_default_prototype` API 추가

---

## 2026-02-16

### Loader: Linear Sweep Function Discovery

`discover_functions_by_linear_sweep()`: 코드 섹션 선형 분석, 프롤로그 패턴으로 함수 경계 추론.
x86/x64 지원, stripped PE 함수 복원률 개선.

---

## 2026-02-15

### Track A+B: Normalizer A-1~A-6 + TypePropagator MinGW (x64 92.1% → 98.8%)

- TypePropagator: MinGW underscore prefix 처리, `propagateAcrossReturns` + integer-cast stripping
- x64 최종: **98.8%** vs Ghidra

---

## 2026-02-14

### Track B: MinGW x86 Binary + x86 Benchmark Suite (baseline 80.0%)

- `comparison_test_x86.exe` 추가 (MinGW i686-w64-mingw32-gcc -O1)
- `suite_x86.yaml` 정의 (8개 함수)
- x86 baseline: 80.0%

---

## 2026-02-13 ~ 02-12

### Type Propagation + Pcode Optimizer + Performance

- `propagate_from_call`: DF-1 fallback, 8B 파라미터 독립 인덱스
- `WhileToForConverter`: loop-init 직전 할당 감지 → `for` 변환
- Performance Group 1-3: hash tuning, static regex, 2-barrier, string cache, O(n) CFG, lazy arch init

---

## 2026-02-11

### Ghidra Integration: Pcode Optimizer Phase 1+2 + Constant Substitution

- 32개 Pcode 최적화 규칙 (Ghidra 142개 대비 23% 커버리지)
- Context-Aware Constant Substitution: 16개 enum 그룹, 100+ Windows API 매핑

---

## 2026-01-20

### Listing View + C++ RTTI Recovery

- `egui_extras::TableBuilder` 기반 가상 스크롤 listing
- RTTI 구조체 파싱, 가상 함수 테이블 복원, 간접 호출 해석

---

## 2025-12-01 ~ 2026-01-19 — Initial Feature Set

- Ghidra FFI (zero-copy, libdecomp.dylib / decomp.dll)
- iced-x86 Pure Rust 디스어셈블러
- Cross-Platform Loader: PE · ELF · Mach-O
- CFG Analysis: dominator tree, loop detection, DOT/JSON 내보내기
- FID Signatures: 40+ CRT 함수
- GDT Type Loading: 5,700+ structures, 6,500+ typedefs
- Time Travel Debugging (Windows)
- TUI Mode (Ratatui)
- Plugin System (동적 Rust 플러그인)

---

## Benchmark Summary

| Platform | Score | Status |
|----------|-------|--------|
| x64 (Windows PE) | **98.8%** | Target 98%+ ✅ |
| x86 MinGW (Windows PE) | **92.6%** | Target 90%+ ✅ |

| Metric | Value |
|--------|-------|
| Pcode optimizer rules | 32 |
| Windows API mappings | 100+ (9 DLLs) |
| GDT structures / typedefs | 5,700+ / 6,500+ |
| CRT/library signatures | 40+ |
| Supported platforms | Windows · Linux · macOS |
