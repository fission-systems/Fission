# Changelog

All notable changes to the Fission project (November 2025 – Present).

---

## 2026-03-07

### 디컴파일러 성능 최적화 + 성공률 개선

putty.exe(2,756개 함수) 전체 벤치마크 기준 **성공률 61% → 87%**, 단일 함수 디컴파일 시간 **207ms → 44ms** 달성.

#### Performance: PostProcessPipeline 계측 및 병목 제거

pass별 `std::chrono` 타이밍 계측을 추가하여 병목을 정밀 식별 후 최적화.

| Pass | Before | After | 개선 |
|------|--------|-------|------|
| compound ops (`+=`, `++` 등) | 22.9ms | 0.01ms | **2,290x** |
| GUID 치환 | 34ms | ~0ms† | O(M·N) → O(N) 스캔 |
| CFGStructurizer (goto 없는 함수) | 48ms | 0.12ms | **400x** |
| PostProcessor::process 합계 | 83ms | 1.4ms | **59x** |

† GUID 로딩은 첫 호출에서만 파일 I/O 발생 (~27ms 일회성)

#### Performance: `convert_while_to_for` — 7 regex → 단일 패스 수동 매칭

`x = x + 1;` → `x++;` 류의 복합 대입 변환을 7개 `regex_replace` 순차 실행에서
O(N) 단일 패스 수동 파서로 전환.

- **수정 위치**: `ghidra_decompiler/src/decompiler/PostProcessor.cc`

#### Performance: GUID 치환 O(M·N) → O(N)

전체 GUID 맵(수천 개)을 순회하며 `string::find`를 반복하는 방식에서,
코드에서 GUID 패턴(8-4-4-4-12)을 먼저 스캔 후 해시맵 한 번 lookup으로 전환.

- **수정 위치**: `ghidra_decompiler/src/processing/passes/ConstantReplacementPasses.cc`

#### Performance: CFGStructurizer Early Exit

`goto` 키워드가 없는 함수에서 13개 CFG 변환 pass를 전부 스킵.
대부분의 함수에는 goto가 없으므로 가장 임팩트가 큰 최적화.

- **수정 위치**: `ghidra_decompiler/src/decompiler/CFGStructurizer.cc`

#### Performance: `LabelAnalyzer::remove_unused_labels` — 동적 regex 제거

미사용 라벨마다 `std::regex`를 새로 컴파일하던 방식에서
O(N·L) 수동 라인 스캐너로 전환.

- **수정 위치**: `ghidra_decompiler/src/decompiler/cfg/LabelAnalyzer.cc`

#### Fix: Recursive Decompilation 에러 715건 수정 (성공률 61% → 87%)

`--decomp-all` 모드에서 callee 분석 시 A→B→A 순환 참조가 발생하면
`isProcStarted()` 체크가 예외를 던져 외부 함수 전체 실패하던 버그 수정.

예외 전파 대신 순환 진입 탐지 시 forward-declaration 스텁을 반환하도록 변경.

- **수정 위치**: `ghidra_decompiler/src/decompiler/DecompilationCore.cpp`
- `ctx->analyzed_callees`에 순환 주소 즉시 등록하여 재시도 방지

#### Fix: Duplicate VariablePiece 예외 처리 강화 (PrototypeEnforcer + Ghidra Merge 충돌)

PrototypeEnforcer가 주입한 엄격한 API 시그니처와 Ghidra Merge가 충돌할 때 발생하는
`LowlevelError: Duplicate VariablePiece`를 Upstream 코드 수정 없이 방어.

- **FFI**: `ghidra::LowlevelError` 전용 catch 추가 — `std::exception` 미상속으로 누락되던 실제 에러 메시지 노출
- **DecompilationCore**: DVP 발생 시 `seed_before_action` 없이 1회 재시도
- **run_analysis_passes**: DVP 발생 시 빈 `AnalysisArtifacts`로 폴백하여 기본 디컴파일 유지

자세한 원인/현재 한계는 `docs/analysis/KNOWN_ISSUES.md` 참조.

- **수정 위치**: `DecompilationCore.cpp`, `libdecomp_ffi.cpp`

#### Infra: 공정 배치 벤치마크 시스템 구축

단일 프로세스 `--decomp-all --benchmark` 모드를 활용, 초기화 비용을 분리한
공정한 Fission vs PyGhidra 비교 스크립트.

- **추가 위치**: `scripts/test/batch_benchmark/runner_fission.py`, `main.py`

#### 전체 변경 파일

| 파일 | 변경 내용 |
|------|----------|
| `ghidra_decompiler/src/decompiler/PostProcessPipeline.cpp` | pass별 chrono 타이밍 계측 추가 |
| `ghidra_decompiler/src/decompiler/PostProcessor.cc` | `convert_while_to_for` 수동 패스 전환, 내부 pass 타이밍 추가 |
| `ghidra_decompiler/src/decompiler/CFGStructurizer.cc` | goto early exit 추가 |
| `ghidra_decompiler/src/decompiler/cfg/LabelAnalyzer.cc` | `remove_unused_labels` 동적 regex 제거 |
| `ghidra_decompiler/src/decompiler/DecompilationCore.cpp` | recursive decompilation throw → stub 반환 |
| `ghidra_decompiler/src/processing/passes/ConstantReplacementPasses.cc` | GUID 치환 O(N) 스캔 전환 |
| `ghidra_decompiler/src/processing/passes/NamingStandardizers.cc` | non-static regex → `static const` |
| `ghidra_decompiler/src/processing/passes/CppVirtualCallPasses.cc` | non-static regex → `static const` |
| `ghidra_decompiler/include/fission/ffi/DecompContext.h` | `analyzed_callees` 캐시 추가 |
| `ghidra_decompiler/src/decompiler/AnalysisPipeline.cpp` | callee 재분석 캐시 적용 |
| `scripts/test/batch_benchmark/runner_fission.py` | 단일 프로세스 배치 모드 재작성 |
| `scripts/test/batch_benchmark/main.py` | 공정 비교 출력 포맷 업데이트 |

#### 벤치마크 결과 (putty.exe, 2,756 functions)

| 지표 | Before | After |
|------|--------|-------|
| 성공률 | 1,693/2,756 (61%) | **2,408/2,756 (87%)** |
| 단일 함수 TOTAL | 207ms | **44ms** |
| postproc TOTAL | 125ms | **32ms** |

---

## 2026-03-03

### 보안 취약점 대응 (2차: 정책 기준선/CI 게이트 정착)

공급망 취약점 대응을 운영 가능한 형태로 고정하고, CI에서의 보안 점검을
기본 품질 게이트로 복원.

#### Added

- `docs/build/SECURITY_ADVISORIES.md` 추가
  - Rust/Node 보안 점검 명령, no-fix advisory 기준선 운영 원칙, 재검토 조건 문서화

#### Changed

- `deny.toml`
  - no safe upgrade가 없는 생태계 advisory를 `advisories.ignore` 기준선으로 명시
  - 신규/패치 가능한 advisory는 CI 실패로 triage 강제
- `.github/workflows/ci.yml`
  - Rust advisory 단계의 non-blocking 설정 제거(`continue-on-error` 제거)
  - 기준선 외 신규 취약점은 build failure로 처리

#### Security Notes

- 본 기준선은 영구 면제가 아니며, upstream 안전 업그레이드 또는 런타임 전환 시 제거 대상.

### 안정화/성능/이식성 리팩토링 완료 (Phase 2 ~ 4)

이번 배치에서 에러 처리 안전성(Phase 2), postprocess 성능(Phase 3), 경로 이식성(Phase 4)을
연속적으로 완료하고 `main`에 반영.

#### Added

- Postprocess 실행 통계 API 추가
  - `PassExecutionStats` (`executed_passes`, `borrowed_outputs`, `owned_outputs`, skip 카운트)
  - `PassRegistry::execute_all_with_stats(...)`
  - `execute_default_passes_with_stats(...)`

#### Changed

- **Phase 2 (`unwrap/expect` 제거):**
  - analysis/loader/pcode/disasm/core/ffi/tauri 전반 panic-prone 경로 제거
  - 실패 경로를 명시적 분기/에러 전파로 통일
- **Phase 3 (`Cow` 기반 문자열 최적화):**
  - pass pipeline을 `Result<Cow<str>, PassError>` 중심으로 전환
  - cleanup/structure/naming/type/dwarf/arithmetic/loop/switch/casts/boilerplate 패스에 no-op `Borrowed` fast path 확장
  - 불필요한 `String` 할당 감소
- **Phase 4 (하드코딩 경로 제거):**
  - `build.rs`의 Windows 절대 경로(`C:\\...`) fallback 제거
  - `VCPKG_ROOT`/`VCPKG_INSTALLATION_ROOT` + 환경 기반 탐색으로 통일
  - DIE 시그니처 로딩을 고정 상대경로 배열 대신 실행경로 기반 상향 탐색으로 변경

#### Fixed

- `switch` 재구성 회귀 수정:
  - `result = ...` 같은 일반 대입 타깃도 인식하도록 패턴 매칭 보완
  - 실패하던 `test_switch_from_if_else_assign_multiline` 복구

#### 검증

- `cargo build -p fission-loader -p fission-analysis -p fission-ffi` 통과
- `cargo test --workspace` 최종 통과 (postprocess 회귀 포함)

#### 대표 커밋

- `719050e` Constants library and initial stability improvements
- `58e05b2` Phase 2.8 remaining unwrap/expect 제거
- `1c6a278` Phase 3.1 Cow 기반 pass pipeline 도입
- `bb5e586` pass execution stats API 추가
- `65888f9` Phase 4 하드코딩 경로 제거
- `d12316d` switch reconstruction 회귀 수정

---

## 2026-03-02

### Decompiler Postprocess 모듈화 리팩토링 (대형 `postprocess.rs` 분해)

`crates/fission-analysis/src/analysis/decomp/postprocess.rs`의 단일 대형 구현을
기능 책임별 하위 모듈로 분리해 유지보수성과 회귀 안정성을 개선.

#### 주요 변경

- `naming` 분리: `replace_field_offsets`, `recognize_swift_accessors`, `demangle_swift_symbols`, `rename_induction_vars`, `rename_semantic_vars`, `apply_dwarf_names`
- `structure` 분리: `simplify_if_structure`, `while_true_to_while_cond`
- `arithmetic` 확장 분리: `apply_arithmetic_idioms`, `recover_divisor`, `mul_pow2_to_shift`
- 공통 조건 유틸 분리: `negate_condition` → `postprocess/condition.rs`
- 테스트 분리: 인라인 테스트 제거, `postprocess/tests.rs`로 이관

#### 신규/변경 파일

- `crates/fission-analysis/src/analysis/decomp/postprocess.rs`: 오케스트레이션 중심으로 슬림화
- `crates/fission-analysis/src/analysis/decomp/postprocess/naming.rs`: 네이밍/필드/DWARF 후처리 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/structure.rs`: if/while 구조 단순화 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/arithmetic.rs`: 산술 이디엄/매직디비전 복구 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/condition.rs`: `negate_condition` 공용 유틸
- `crates/fission-analysis/src/analysis/decomp/postprocess/tests.rs`: postprocess 회귀 테스트 모듈
- `docs/analysis/POSTPROCESS_MODULES.md`: 모듈 구조/확장 규칙 문서 추가

#### 테스트/검증

- `cargo build -p fission-analysis` 통과
- `cargo test -p fission-analysis postprocess::tests:: -- --nocapture` 통과 (`test_switch_from_if_else_assign_multiline`, `test_negate_condition_basic_cases`, `test_while_true_to_while_cond_simple`)

---

## 2026-03-01

### 디컴파일러 품질 대폭 개선: 4대 버그 수정 + v4 벤치마크 시스템

**벤치마크 점수: ARM64 69.4% → 88.9% | x64 71.6% → 88.2% | Linux 69.8% → 91.6% | Windows 68.0% → 91.1%**

#### Bug 1 (CRITICAL): 빈 함수 본문 생성 버그 수정 (`parse_decl_varname`)

`normalize_msvc_crt_printf` 후처리 스텝의 `parse_decl_varname` 람다가 `return param_1 / 3;`를 고아 변수 선언으로 잘못 파싱하는 치명적 버그 수정.

- **원인**: `return` 키워드 이후의 표현식에서 `3`을 변수명으로 파싱 → 다른 곳에서 미사용으로 판단 → 해당 라인 전체 삭제 → 함수 본문 소실
- **증상**: `divide_by_3(unsigned x)` → `uint __Z11divide_by_3j() { }` (파라미터도, 본문도 없음)
- **수정 위치**: `ghidra_decompiler/src/processing/PostProcessors.cc`
  - `parse_decl_varname`: `return` 키워드 검사 + 숫자로 시작하는 식별자 거부 조건 추가
  - `parse_assign_lhs`: `return` 키워드 exclusion 추가

#### Bug 2: 소멸자 Segfault 수정 (`arch.release()`)

- **원인**: `DecompContext::~DecompContext()`에서 `arch.reset()` 호출 시 Ghidra `Architecture` 소멸자 체인이 SIGSEGV 발생 (try/catch로 포착 불가)
- **수정**: `arch.reset()` → `arch.release()` (제어된 메모리 누수, 프로세스 종료 시 OS가 회수)
- **수정 위치**: `ghidra_decompiler/src/ffi/DecompContext.cpp`

#### Bug 3: 구조체 포인터 접근 미변환 수정 (`convert_struct_access`)

- **원인**: 기존 `annotate_structure_offsets`는 주석만 추가, `*(type*)(param + offset)` → `param->field` 변환 불가. 추가로 hex 리터럴 분할 버그(`0xc` → `0  /* field_0 */xc`) 존재
- **수정**: 완전한 새 함수 `convert_struct_access()` 구현 (5단계: 구조체 typedef 파싱 → struct 타입 파라미터 탐지 → 포인터 역참조 변환 → 오프셋-0 `*param` 변환 → 잔여 오프셋 재주석)
- **수정 위치**: `ghidra_decompiler/src/processing/PostProcessors.cc`, `ghidra_decompiler/include/fission/processing/PostProcessors.h`

#### Bug 4: 복합 대입 연산자 regex가 `->` 를 삼키는 버그 수정

- **원인**: `sub_assign_pattern` 정규식 `(\w+)\s*=\s*\1\s*-\s*([^;]+);`이 `local_8 = local_8->field_8;`를 빼기 연산으로 매칭 → `local_8 -= >field_8` 생성
- **수정**: `->` 음수 전방탐색(negative lookahead) 추가
- **수정 위치**: `ghidra_decompiler/src/decompiler/PostProcessor.cc` (2곳)

#### Feature: v4 벤치마크 시스템 구축

7개 C++ 소스 파일 × 4개 플랫폼 × 2개 최적화 레벨 = **56개 테스트 바이너리** 기반 자동화 벤치마크.

| 구성 요소 | 내용 |
|-----------|------|
| 테스트 소스 | arithmetic_idioms, control_flow, structs_classes, calling_conventions, string_memory, real_world_algorithms, advanced_patterns |
| 플랫폼 | macOS ARM64, macOS x86_64, Linux x86_64, Windows x86_64 |
| 스크립트 | `scripts/benchmark/run_benchmark.sh`, `benchmark_v4.py`, `analyze_results.py` |
| YAML 수트 | `suite_macos_arm64.yaml`, `suite_macos_x86_64.yaml`, `suite_linux_x86_64.yaml`, `suite_windows_x86_64.yaml` |

- **패턴 OR 지원**: `benchmark_v4.py`에 `|` 연산자 지원 추가 (예: `%|&`로 ARM64 `%` 와 x64 `&` 동시 매칭)

#### 벤치마크 결과 요약

| 플랫폼 | Bug 1+2 수정 후 | Bug 3 수정 후 | 최종 |
|--------|----------------|---------------|------|
| macOS ARM64 | 69.4% → 85.9% (+16.5pp) | 85.9% → 88.9% (+3.0pp) | **88.9%** |
| macOS x86_64 | 71.6% → 87.6% (+16.0pp) | 87.6% → 88.2% (+0.6pp) | **88.2%** |
| Linux x86_64 | 69.8% → 88.6% (+18.8pp) | 88.6% → 91.6% (+3.0pp) | **91.6%** |
| Windows x86_64 | 68.0% → 88.0% (+20.0pp) | 88.0% → 91.1% (+3.1pp) | **91.1%** |

#### 변경 파일 목록

| 파일 | 변경 내용 |
|------|----------|
| `ghidra_decompiler/src/processing/PostProcessors.cc` | Bug 1 parse_decl_varname 수정, Bug 3 convert_struct_access 구현 |
| `ghidra_decompiler/include/fission/processing/PostProcessors.h` | convert_struct_access 선언 추가 |
| `ghidra_decompiler/src/ffi/DecompContext.cpp` | Bug 2 arch.release() 수정 |
| `ghidra_decompiler/src/decompiler/PostProcessor.cc` | Bug 4 복합대입 regex 수정 |
| `ghidra_decompiler/src/decompiler/PostProcessPipeline.cpp` | 파이프라인 정리 |
| `scripts/benchmark/benchmark_v4.py` | OR 패턴(`\|`) 지원 추가 |
| `scripts/benchmark/suites/*.yaml` | mod 함수 패턴 `%\|&` 업데이트 (플랫폼 차이 흡수) |
| `examples/sources/test_*.cpp` | 7개 테스트 소스 추가 |

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
