# Roadmap

이 문서는 현재 Fission의 **중기 우선순위**를 정리한 것이다.  
세부 아이디어 메모는 `docs/idea/*`, 심화 분석은 `docs/analysis/*`에 남기되, 실제 우선순위 판단은 이 문서를 기준으로 본다.

## Current Direction

현재 Fission의 큰 방향은 다음과 같다.

1. **legacy 경로 안정 유지**
   - Ghidra native decompilation + Fission postprocess
   - 내부 fallback / compat baseline 용도로 축소 유지
2. **mlil-preview 경로 확장**
   - Ghidra p-code를 입력으로 받는 Fission NIR/HIR + Rust printer
   - 제품 기본 경로로 고정하는 단계
3. **문서/벤치/지표 정리**
   - preview / native / assembly fallback을 분리 측정
   - 품질 개선과 legacy 퇴역 기준을 수치로 관리

## Near-Term Priorities

### 1. Legacy Deprecation Inventory + Preview-First Routing

현재 우선순위는 legacy를 삭제하는 것이 아니라, preview-first를 제품 기본 정책으로 고정하고 legacy를 explicit fallback / compat 모드로만 남기는 것이다.

핵심 항목:
- GUI에서는 legacy 선택지를 더 이상 노출하지 않기
- CLI `--engine legacy`는 숨은 compat 모드로만 유지
- fallback taxonomy를 다음 수준으로 고정
  - `preview_timeout`
  - `preview_unsupported`
  - `native_pcode_failure`
  - `legacy_fallback`
  - `assembly_fallback`
- 아직 legacy baseline이 필요한 코퍼스/함수 목록을 문서와 artifact에 유지

현재 inventory 기준 legacy 개입 지점:
- `fission-static select_preview_output()`에서 preview skip/error 후 native decompile fallback
- CLI oneshot decompile에서 explicit legacy compat mode와 preview rescue / assembly fallback 분기
- Tauri GUI decompile에서 preview 실패 후 native decompile 또는 explicit assembly/native failure surface
- benchmark/compare 스크립트에서 legacy baseline과 fallback kind taxonomy를 함께 집계

legacy 삭제 전 조건:
- fixed-seed 코퍼스에서 preview direct 또는 explicit native/assembly fallback만으로 재현 가능한 결과를 확보
- `putty`, `everything`, `WinMerge`, `EverPlanet`, `ida76sp1` watchlist 함수가 hang 없이 종료
- legacy baseline이 아직 필요한 코퍼스와 함수 목록을 별도 benchmark artifact로 계속 추적

### 2. Type Recovery / Type Failure 감소

현재 가장 분명한 품질 병목은 구조화가 아니라 `type` 계열 실패다.

현재 대표 hard case:
- `putty 0x1400052b0`
- `putty 0x140006380`
- `cmkr 0x140002cc0`

다음 라운드 핵심 목표:
- legacy type failure 감소
- fallback 품질 유지
- preview 경로와 충돌하지 않는 type recovery 개선

### 3. `mlil-preview` Coverage 확대

v13 기준 preview는 의미 있는 수준까지 올라왔지만 아직 full replacement는 아니다.

우선순위:
- 더 많은 multi-block 함수 직접 처리
- loop/header normalization 강화
- label/goto fallback 비율 감소
- `switch`와 더 복잡한 CFG 처리 범위 확대

### 4. Preview Quality 향상

현재 preview가 이미 잘하는 것:
- short-circuit folding
- loop lowering
- cast canonicalization
- `PIECE/SUBPIECE` recombination

다음 개선 포인트:
- type-aware expression quality
- 더 나은 aggregate handling
- preview 전용 idiom recognition 확대
- large-function readability 개선

## Medium-Term Direction

### 1. Fission-owned Decompiler Stack 강화

장기적으로는 아래 구조를 더 강화하는 것이 핵심이다.

- Ghidra: lift / CFG / baseline type recovery / fail containment
- Fission NIR: normalization / stack abstraction / temp coalescing
- Fission HIR: structured pseudocode
- Rust printer: 최종 출력

즉, 목표는 “Ghidra를 더 잘 후처리하는 도구”가 아니라
**Ghidra를 하부 엔진으로 사용하는 Fission 고유 디컴파일러**다.

### 2. Preview를 제품 기본 경로로 고정

중기 목표:
- preview 채택률 상승
- preview 출력 품질 안정화
- legacy를 explicit fallback / compat path로만 남기기

## GUI / Product

### Tauri 중심 유지

현재 제품 GUI 기준은 Tauri다.

남은 우선순위:
- preview-first decompile UX 유지
- native / assembly fallback 종류가 명확한 표시 유지
- dynamic debug / timeline은 별도 트랙으로 계속 진행

### egui 문서 정리

- `docs/gui/GUI_GUIDE.md`는 현재 기준 source of truth가 아니다.
- 장기적으로는 Tauri 기준 문서만 남기고 legacy GUI 문서는 축소 또는 archive 방향 검토

## Docs / Benchmark

### 문서 체계 정리

상위 기준 문서:
- [`/Users/sjkim1127/Fission/README.md`](/Users/sjkim1127/Fission/README.md)
- [`/Users/sjkim1127/Fission/docs/README.md`](/Users/sjkim1127/Fission/docs/README.md)
- [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
- [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)

### 벤치마크 관리

계속 유지할 원칙:
- preview / native / assembly fallback 지표 분리
- raw JSON은 artifact로, 요약본은 문서로
- `everything`, `putty`, `cmkr` 회귀 세트 유지

추가 코퍼스:
- `ida76sp1`
  - x64 멀티-DLL C++/plugin corpus
  - 목적: large C++ GUI + shared DLL + plugin ecosystem regression
  - 향후 확장: cross-image symbol/type propagation 실험용

## Out of Scope For Now

당장 우선순위가 아닌 항목:
- semantic renaming 전면화
- xref sync / GUI interaction polish 대규모 개편
- 전면적인 성능 최적화 재진입
- Ghidra core를 대규모 수정하는 방향

## Related Docs

- [`/Users/sjkim1127/Fission/docs/FEATURES.md`](/Users/sjkim1127/Fission/docs/FEATURES.md)
- [`/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md`](/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md)
- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)
