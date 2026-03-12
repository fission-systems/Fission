# Architecture

이 문서는 현재 Fission의 **실제 아키텍처 기준 문서**다.  
특히 2026-03 기준으로는 `legacy`와 `mlil-preview` 두 개의 디컴파일 경로가 공존하므로, 역할 분리를 명확히 이해하는 것이 중요하다.

## Top-Level Model

Fission은 크게 네 층으로 본다.

1. **Binary / metadata layer**
2. **Native lifting / decompilation layer**
3. **Fission-owned analysis and IR layer**
4. **Presentation layer**

핵심 방향은 다음과 같다.

- Ghidra는 점점 더 **lifting / CFG / baseline type recovery / fail containment** 역할로 축소
- Fission은 그 위에서 **NIR/HIR + Rust printer**를 통해 더 읽기 좋은 pseudocode를 직접 생성

## Workspace Structure

주요 crate 역할:

- `fission-core`
  - 공통 에러, 설정, 모델
- `fission-loader`
  - PE / ELF / Mach-O 로딩
  - 심볼 / 문자열 / 메타데이터 추출
- `fission-disasm`
  - 디스어셈블리 표면 계층
- `fission-signatures`
  - WinAPI / type / signature DB
- `fission-ffi`
  - Rust ↔ native decompiler 경계
- `fission-analysis`
  - legacy decompile wrapping
  - postprocess
  - CFG / xref / debug / unpacking
- `fission-pcode`
  - p-code 모델
  - optimizer
  - preview NIR/HIR
  - Rust pseudocode printer
- `fission-cli`
  - CLI 엔트리포인트
- `fission-tauri`
  - Tauri frontend/backend

## Dependency Direction

기본 흐름:

`fission-core`
→ `fission-loader` / `fission-signatures` / `fission-disasm`
→ `fission-pcode`
→ `fission-analysis`
→ `fission-cli` / `fission-tauri`

native 경계:

`ghidra_decompiler/src/*`
↔ `fission-ffi`
↔ `fission-analysis`

원칙:
- `fission-ffi`가 unsafe/native boundary를 소유
- UI/CLI는 orchestration만 담당
- 핵심 품질 로직은 analysis / pcode 레이어에 둠

## Decompilation Architecture

### 1. Legacy Path

현재 가장 안정적인 경로.

흐름:
- Ghidra native decompiler
- Fission postprocess (`fission-analysis`)
- 최종 C-like 출력

역할:
- 실전 기본 경로
- regression guard
- type promotion, cleanup, control-flow cleanup의 현재 기준선

### 2. MLIL Preview Path

차세대 경로.

흐름:
- Ghidra p-code lifting
- `fission-pcode` optimizer
- Fission NIR builder
- Fission HIR lowering
- Rust printer
- unsupported case는 preview pseudocode fallback 또는 legacy fallback

현재 목적:
- Ghidra C 출력 후처리로는 도달하기 어려운 가독성 개선
- 구조화와 정규화를 Fission이 직접 통제

현재 지원 범위:
- PE x64 only
- stack-slot recovery
- multi-block `if`
- multi-block `if/else`
- short-circuit `&&`, `||`
- multi-block `while`
- multi-block `do-while`
- cast canonicalization
- `PIECE` / `SUBPIECE` recombination

## Runtime Layers

### Binary / Static Analysis Layer

- binary parsing
- section / import / export / string extraction
- disassembly
- signature lookup
- xref / CFG support

### Native Decompiler Layer

native decompiler preparation의 단일 진입점은:

- `fission_analysis::analysis::decomp::prepare_native_decompiler_for_binary`

이 경로에서 처리되는 것:
- binary image load
- memory block registration
- symbols / sections
- known functions
- FID DB
- GDT
- timeout / logging setup

원칙:
- CLI와 GUI는 이 단일 경로만 사용
- step-specific 에러 정제도 이 경로에서 통합 관리

### Fission IR / Quality Layer

현재 두 갈래가 공존한다.

- `fission-analysis` postprocess
  - legacy 전용 품질 계층
- `fission-pcode` NIR/HIR
  - preview 전용 품질 계층

장기 방향:
- 후처리 문자열 정리보다, IR 레벨 구조화와 normalization을 중심으로 이동

### Presentation Layer

- `fission-cli`
  - one-shot decompilation
  - benchmark
  - engine selection
- `fission-tauri`
  - assembly / decompile view
  - engine selector
  - fallback badge / engine badge

## Engine Modes

현재 제품 노출 엔진:

- `legacy`
- `mlil_preview`
- `auto`

정책:
- `legacy`: 안정성 우선
- `mlil_preview`: Fission NIR/HIR 전용 경로
- `auto`: low-risk subset에서는 preview 우선 시도, 실패 시 legacy fallback

## Error / Fallback Policy

원칙:
- 틀린 고수준 코드를 억지로 만들지 않는다
- 실패 시 label/goto pseudocode 또는 legacy fallback으로 안전하게 내려간다
- `Duplicate VariablePiece` 같은 hard case는 완전 수정 전까지 fallback 안정성을 우선한다

## Logging / Diagnostics

제어 surface:
- `[decompiler].log_verbose`
- `[decompiler].log_file`
- CLI `--verbose`

에러 경로:
- native `last_error`
- Rust `FissionError`
- benchmark failure classification

## Native Code Boundary Rule

`ghidra_decompiler/decompile`는 upstream 성격이 강한 영역이다.  
가능하면 `ghidra_decompiler/src/*`와 Rust 쪽에서 통합/안정화 작업을 하고, upstream `decompile` 내부는 꼭 필요할 때만 건드린다.

## Current Architectural Direction

현재 Fission의 핵심 방향은 이것이다.

- Ghidra를 “최종 디컴파일러”로 쓰는 것이 아니라,
- **좋은 lifting / CFG / type recovery backend**로 사용한다
- 그 위에서 Fission이 자체 NIR/HIR와 printer를 만들어
- 더 읽기 좋은 pseudocode를 생성한다

즉, Fission은 더 이상 “Ghidra output post-processor”만이 아니라,
**Ghidra를 하부 엔진으로 사용하는 독자 디컴파일러 아키텍처**로 이동 중이다.
