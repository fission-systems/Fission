# Features

이 문서는 현재 Fission이 **실제로 제공하는 기능**을 최신 기준으로 요약한다.  
상세 구조는 [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md), 최신 변경 이력은 [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)를 기준으로 본다.

## Decompilation Engines

Fission에는 현재 두 개의 디컴파일 경로가 있다.

### `legacy`

Ghidra native decompiler + Fission 후처리 파이프라인.

현재 가장 안정적인 경로이며, serious analysis의 기본 품질 기준이다.

주요 기능:
- WinAPI 시그니처 기반 type promotion
- `CONCAT` / piece residue 정리
- `goto` 감소 및 CFG 구조화
- switch clustering
- temp inlining
- stack / piece access 정규화

### `mlil-preview`

Ghidra p-code를 받아 Fission NIR/HIR + Rust printer로 직접 pseudocode를 생성하는 차세대 경로.

현재 지원:
- PE x64 only
- stack-slot recovery
- multi-block `if`
- multi-block `if/else`
- short-circuit `&&` / `||`
- multi-block `while`
- multi-block `do-while`
- cast canonicalization
- `PIECE` / `SUBPIECE` recombination
- preview 전용 label/goto fallback

현재 한계:
- 범용 품질은 아직 `legacy`보다 낮을 수 있음
- 일부 large function / type-heavy 함수는 fallback 필요
- field name 추측, semantic renaming은 아직 하지 않음

## Binary / Architecture Support

지원 포맷:
- PE
- ELF
- Mach-O

지원 아키텍처:
- x86
- x86-64
- ARM64 / AArch64

단, `mlil-preview`의 현재 1차 범위는 **PE x64 only**다.

## Analysis / Recovery Capabilities

정적 분석 계층이 제공하는 핵심 기능:
- function discovery
- imports / exports / strings / sections
- disassembly
- xref / CFG 기반 분석
- p-code optimization
- signature / type DB 로딩
- FID 기반 심볼 식별

## Type / Signature Features

현재 tree에 있는 타입/시그니처 기능:
- Windows signature DB (`fission-signatures`)
- WinAPI prototype injection
- structure / pointer type promotion
- GDT loading
- baseline type propagation

현재 강한 경로:
- `LPRECT`, `RECT`, `LPMSG` 같은 WinAPI 구조체 포인터 승격
- legacy path의 parameter / structure cleanup

## CLI Features

현재 CLI가 제공하는 주요 기능:
- binary info
- function list
- strings
- disassembly
- single-function decompilation
- batch decompilation
- benchmark mode
- engine selection

핵심 옵션:
- `--profile balanced|quality|speed`
- `--engine legacy|mlil-preview|auto`
- `--timeout-ms`
- `--benchmark`
- `--ghidra-compat`

## Desktop GUI Features

현재 Tauri GUI에서 실제로 제공하는 기능:
- function list / filtering
- assembly tabs
- decompile tabs
- decompiler options dialog
- engine selector (`legacy`, `mlil_preview`, `auto`)
- engine used / fallback badge
- strings / imports / exports / search / CFG 관련 패널

주의:
- [`/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md`](/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md)는 현재 Tauri UI 기준 문서가 아니라, 오래된 egui 문서다.

## Benchmark Snapshot

체크인된 대표 benchmark summary:
- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)

최근 품질 작업에서 확인된 방향:
- `legacy`는 안정적인 기본 경로
- `mlil-preview`는 coverage와 구조화 품질이 빠르게 올라가는 중
- preview benchmark에서는 `goto`와 temp surface가 크게 줄어드는 방향이 확인됨

## Known Limits

현재 문서 기준으로 중요한 제한:
- `mlil-preview`는 아직 full replacement가 아님
- 일부 `type` 계열 함수는 legacy에서도 hard case로 남아 있음
- semantic renaming, advanced field naming, perfect high-level idiom recovery는 아직 진행 중

## Related Docs

- [`/Users/sjkim1127/Fission/docs/README.md`](/Users/sjkim1127/Fission/docs/README.md)
- [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
- [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)
- [`/Users/sjkim1127/Fission/docs/ROADMAP.md`](/Users/sjkim1127/Fission/docs/ROADMAP.md)
