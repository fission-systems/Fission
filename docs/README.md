# Fission Docs Index

이 문서는 현재 Fission 문서의 **진입점**이다.  
최신 기준으로 보면 문서는 아래 우선순위로 읽는 것이 가장 효율적이다.

## Start Here

1. [`/Users/sjkim1127/Fission/README.md`](/Users/sjkim1127/Fission/README.md)
   - 저장소 개요
   - 현재 지원 경로
   - 빌드 / 실행 빠른 시작
2. [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
   - 현재 아키텍처
   - `legacy` / `mlil-preview` 경로 차이
   - crate/layer 책임 분리
3. [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)
   - 최신 변경 내역
   - 품질/성능 개선 이력
4. [`/Users/sjkim1127/Fission/docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md)
   - 플랫폼별 빌드 가이드

## Current Source Of Truth

현재 문서의 기준은 아래다.

- 제품/워크스페이스 개요: [`/Users/sjkim1127/Fission/README.md`](/Users/sjkim1127/Fission/README.md)
- 시스템 구조: [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
- 기능 요약: [`/Users/sjkim1127/Fission/docs/FEATURES.md`](/Users/sjkim1127/Fission/docs/FEATURES.md)
- 중장기 방향: [`/Users/sjkim1127/Fission/docs/ROADMAP.md`](/Users/sjkim1127/Fission/docs/ROADMAP.md)
- 릴리즈 이력: [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)
- 최신 벤치마크 기준: [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)

## Folder Tree

### `/Users/sjkim1127/Fission/docs/architecture`

- 시스템 구조와 crate/layer 책임
- FFI 경계
- decompiler pipeline 역할 분리

대표 문서:
- [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)

### `/Users/sjkim1127/Fission/docs/build`

- 플랫폼별 빌드/런 가이드
- 보안 공지

대표 문서:
- [`/Users/sjkim1127/Fission/docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md)
- [`/Users/sjkim1127/Fission/docs/build/SECURITY_ADVISORIES.md`](/Users/sjkim1127/Fission/docs/build/SECURITY_ADVISORIES.md)

### `/Users/sjkim1127/Fission/docs/changelog`

- 릴리즈별 변경 이력
- 품질/성능 개선 기록

대표 문서:
- [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)

### `/Users/sjkim1127/Fission/docs/benchmark`

- 체크인된 요약 벤치마크
- 디버깅/재현 보조 문서

대표 문서:
- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)
- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.json`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.json)

### `/Users/sjkim1127/Fission/docs/analysis`

- decompiler/postprocess/type propagation/FID 관련 심화 분석 노트
- 실험/검증 문서

이 폴더는 **설계 참고 자료**가 많고, 일부 문서는 현재 구현 상태보다 오래됐을 수 있다.  
최종 기준은 항상 changelog와 architecture 문서를 우선한다.

대표 문서:
- [`/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md`](/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md)
- [`/Users/sjkim1127/Fission/docs/analysis/PASS_SYSTEM.md`](/Users/sjkim1127/Fission/docs/analysis/PASS_SYSTEM.md)
- [`/Users/sjkim1127/Fission/docs/analysis/POSTPROCESS_MODULES.md`](/Users/sjkim1127/Fission/docs/analysis/POSTPROCESS_MODULES.md)

### `/Users/sjkim1127/Fission/docs/cli`

- CLI 동작 방식과 one-shot 모드 설명

대표 문서:
- [`/Users/sjkim1127/Fission/docs/cli/CLI_ONE_SHOT_MODE.md`](/Users/sjkim1127/Fission/docs/cli/CLI_ONE_SHOT_MODE.md)

### `/Users/sjkim1127/Fission/docs/gui`

- GUI 관련 문서

주의:
- [`/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md`](/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md)는 오래된 egui 기준 문서다.
- 현재 실제 제품 GUI 기준은 Tauri 프론트엔드와 루트 README 설명이다.

### `/Users/sjkim1127/Fission/docs/idea`

- 아이디어 스케치
- 조사 메모
- 장기 실험 방향

이 폴더는 **source of truth가 아니다**.  
구현 기준으로 삼기 전에 반드시 architecture / roadmap / changelog와 대조해야 한다.

### `/Users/sjkim1127/Fission/docs/plan`

- 특정 외부 엔진/통합 계획 문서

### `/Users/sjkim1127/Fission/docs/plugins`

- 플러그인 개발 관련 문서

## Recommended Reading Order By Goal

### 저장소를 처음 이해할 때

1. [`/Users/sjkim1127/Fission/README.md`](/Users/sjkim1127/Fission/README.md)
2. [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
3. [`/Users/sjkim1127/Fission/docs/FEATURES.md`](/Users/sjkim1127/Fission/docs/FEATURES.md)

### 빌드/실행이 목적일 때

1. [`/Users/sjkim1127/Fission/docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md)
2. [`/Users/sjkim1127/Fission/README.md`](/Users/sjkim1127/Fission/README.md)

### 디컴파일러 품질 작업을 할 때

1. [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)
2. [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
3. [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)
4. [`/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md`](/Users/sjkim1127/Fission/docs/analysis/KNOWN_ISSUES.md)

### 실험 아이디어를 찾을 때

1. [`/Users/sjkim1127/Fission/docs/ROADMAP.md`](/Users/sjkim1127/Fission/docs/ROADMAP.md)
2. `/Users/sjkim1127/Fission/docs/idea/*`

