# Ghidra vs Fission — Gap Analysis (2026-03-04)

## 개요

이 문서는 Ghidra 11.4.2 디컴파일러와 Fission의 실제 기능 차이를 코드 기반으로 정량화한 분석입니다.  
**중요:** Fission은 Ghidra C++ 코어(`libdecomp`)를 공유 라이브러리로 직접 임베딩합니다.  
따라서 Ghidra의 대부분 분석 기능은 `decomp_function()` 호출 시 이미 실행됩니다.

---

## 아키텍처 — Fission 실행 파이프라인

```
┌─────────────────────────────────────────────────────┐
│  Ghidra libdecomp  (이미 컴파일되어 실행 중)           │
│                                                     │
│  ruleaction.cc   → 135개 Rule  (대수 변환/최적화)    │
│  coreaction.cc   → 65개  Action (분석 패스)          │
│  blockaction.cc  → 12개  BlockAction (CFG 변환)      │
│  heritage.cc     → SSA 구성 / phi-node 삽입          │
│  ActionInferTypes→ 타입 전파 (DFG BFS)               │
│  jumptable.cc    → 7가지 점프 테이블 모델             │
│  printc.cc       → C 코드 출력 (118 virtual methods) │
│                                                     │
└──────────────────────┬──────────────────────────────┘
                       │ Ghidra 출력 후 Fission 독자 레이어
                       ▼
┌─────────────────────────────────────────────────────┐
│  Fission C++ 레이어                                  │
│                                                     │
│  TypePropagator.cc   (2075줄) — Ghidra 보완 타입전파 │
│  StructureAnalyzer.cc (535줄) — 구조체 복원          │
│  CallingConvDetector  (453줄) — 호출 규약 검증       │
│  FidDatabase.cc       (617줄) — FID 시그니처 매칭    │
│  FunctionMatcher.cc   (377줄) — 패턴 기반 매칭       │
│  NoReturnDetector.cc          — noreturn 감지        │
│  VTableAnalyzer.cc            — vtable 분석          │
│  EmulationAnalyzer.cc         — 에뮬레이션 분석      │
│                                                     │
│  PostProcessor.cc    (10패스) — C 텍스트 후처리      │
│  CFGStructurizer.cc  (12패스) — 제어흐름 구조화      │
│  SwitchReconstructor  (4패턴) — switch 복원          │
│                                                     │
└──────────────────────┬──────────────────────────────┘
                       │ FFI
                       ▼
┌─────────────────────────────────────────────────────┐
│  Fission Rust 레이어                                 │
│                                                     │
│  fission-ffi     — C++ ↔ Rust FFI 인프라            │
│  fission-pcode   — Pcode IR + Rust 옵티마이저 (30룰) │
│  fission-loader  — PE/ELF/Mach-O 파싱               │
│  fission-analysis— CFG 분석 (Rust 측)               │
│  fission-tauri   — Tauri GUI                        │
│  fission-cli     — CLI                              │
└─────────────────────────────────────────────────────┘
```

---

## 소스 규모

| 항목 | Fission | Ghidra |
|------|---------|--------|
| C++ 소스 라인 (decompiler) | ~19,259 | ~129,533 |
| Rust 소스 라인 (추가) | ~45,000 | — |
| Rule/Action 서브클래스 | 212개 (Ghidra 코어, 이미 실행) | 212개 |
| Fission 자체 Pcode 룰 (Rust) | 30개 | — |
| Fission TypePropagator 처리 opcode | 16개 | — |

---

## 카테고리별 현황

### ✅ Ghidra 코어가 이미 처리 (Fission에서 실행 중)

| 기능 | 구현체 | 상태 |
|------|--------|------|
| 타입 전파 (ActionInferTypes) | coreaction.cc:4847~ | ✅ 실행 중 |
| SSA 구성 / Heritage 분석 | heritage.cc | ✅ 실행 중 |
| 135개 대수 변환 Rule | ruleaction.cc | ✅ 실행 중 |
| 점프 테이블 분석 (7모델) | jumptable.cc | ✅ 실행 중 |
| 데드코드 제거 | coreaction.cc ActionDeadCode | ✅ 실행 중 |
| 파라미터 복원 | fspec.cc ActionInputPrototype 등 | ✅ 실행 중 |
| C 코드 출력 (PrintC) | printc.cc | ✅ 실행 중 |
| 루프 구조 복원 (while/do-while) | blockaction.cc | ✅ 실행 중 |
| 상수 전파 | ruleaction.cc RuleCollapseConstants | ✅ 실행 중 |
| 공통 부분식 제거 (CSE) | blockaction.cc ActionMultiCse | ✅ 실행 중 |

---

### ⚙️ Ghidra OptionDatabase — Fission에서 미활성화된 옵션

`options.hh`에 정의된 옵션들 중 Fission `ArchInit.cc`/`configure_arch()`에서 아직 설정하지 않은 것들.

| 옵션 클래스 | 효과 | 우선순위 |
|------------|------|---------|
| `OptionNullPrinting` | 포인터 0을 `NULL`로 출력 | 높음 |
| `OptionInPlaceOps` | `x = x + 1` → `x += 1` (복합 대입) | 높음 |
| `OptionHideExtensions` | `(int)(char)x` 형태의 불필요한 확장 연산 숨김 | 높음 |
| `OptionNoCastPrinting` | 안전한 캐스트를 생략 (CastStrategy 활용) | 중간 |
| `OptionCommentHeader` | 함수 상단 헤더 주석 블록 출력 | 중간 |
| `OptionCommentInstruction` | 어셈블리 주소 기반 인라인 주석 | 낮음 |
| `OptionBraceFormat` | 중괄호 스타일 제어 (K&R vs Allman) | 낮음 |
| `OptionToggleRule` | 개별 Rule 켜기/끄기 세밀 제어 | 낮음 |

**적용 방법:** `configure_arch()` 또는 `apply_feature_flags()` 내에서:
```cpp
arch->options->set(ghidra::ELEM_NULLPRINTING.getId(), "on", "", "");
arch->options->set(ghidra::ELEM_INPLACEOPS.getId(), "on", "", "");
arch->options->set(ghidra::ELEM_HIDEEXTENSIONS.getId(), "on", "", "");
```

---

### ❌ Fission 레이어에 완전히 미구현

#### 1. 디버그 심볼 임포트 (난이도: 높음)

| 항목 | 설명 |
|------|------|
| **DWARF 파싱** | `.debug_info`, `.debug_types` 섹션 → 타입/변수명/인라인 함수 확정 |
| **PDB / CodeView 파싱** | MSVC `.pdb` 파일 → 함수 시그니처, 로컬 변수명, 타입 |

현황: 해당 코드 없음. DWARF는 `libdwarf` 또는 `LLVM DebugInfo` 레이어 필요.  
효과: 디버그 빌드 바이너리에서 타입/변수명이 Ghidra 수준으로 즉시 개선.

#### 2. Union 복원 (난이도: 중간)

현황: `StructureAnalyzer.cc`는 단순 offset→필드 매핑만 수행.  
미구현: 동일 base에 겹치는(overlapping) offset 접근 감지 → `union` 타입 선언.  
Ghidra 대응: `unionresolve.cc` (이미 컴파일됨, API는 있으나 Fission에서 활용 안 함).

#### 3. Enum 추론 (난이도: 중간)

현황: 미구현.  
방법: `switch` 문의 case 값 집합 분석 → 연속/불연속 정수값 묶음 → `enum` 타입 생성.  
Ghidra 대응: `TypeFactory::createEnum()` API 사용 가능.

#### 4. 비트필드 복원 (난이도: 중간)

현황: 미구현.  
패턴: `(x >> 3) & 0x1F` 형태의 연속 마스크 연산 → `struct { int a:3; int b:5; }`.

#### 5. C++ 예외 구조 복원 완전화 (난이도: 높음)

현황: `SehCleanup.cc`에 SEH 부분 처리 있음.  
미구현: MSVC C++ EH (`__CxxFrameHandler3`), GCC `_Unwind_*` 기반 DWARF unwind.

#### 6. 콜그래프 기반 타입 전파 (난이도: 높음)

현황: 함수별 독립 분석.  
미구현: 호출자-피호출자 간 타입 피드백 루프 (함수 A의 리턴 타입 → 함수 B의 인수 타입 확정).  
참조: `docs/ROADMAP.md` 1번 항목 (Call-graph 기반 FID 정확도 향상).

#### 7. 아키텍처별 특화 분석 (난이도: 높음)

현황: x86-32/x64 완전 지원. ARM64 일부(호출 규약).  
미구현: ARM32 CFG 특화, MIPS, RISC-V, PowerPC 분석 레이어.  
Ghidra: 38개 ISA 지원.

---

## 이미 Ghidra가 하는데 이전 분석에서 '미구현'으로 잘못 집계된 항목

| 항목 | 실제 상태 |
|------|----------|
| 타입 전파/추론 (14%로 표기됨) | ✅ Ghidra `ActionInferTypes` 전체 실행 중 |
| Pcode 최적화 (14%로 표기됨) | ✅ 212개 Rule/Action 전체 실행 중 |
| CFG 루프 복원 | ✅ `blockaction.cc` 실행 중 |
| 파라미터 타입 복원 | ✅ `ActionPrototypeTypes` 실행 중 |

> **주의:** 이전 수치 분석(35%)은 "Fission이 자체적으로 추가 구현한 것"만 측정했고,  
> Ghidra 코어가 이미 처리하는 부분을 제외한 결과입니다.  
> 실제 완성도는 이보다 훨씬 높습니다.

---

## 실질적 차별화 포인트 (Ghidra 대비)

| 항목 | Fission 고유 강점 |
|------|-----------------|
| 실행 환경 | JVM 없이 네이티브 바이너리 |
| GUI | Tauri + React 19 모던 UI |
| 자동화 | CLI + Rust API 배치 처리 |
| 커스터마이징 | C++/Rust 직접 수정 (Java 플러그인 불필요) |
| FFI 인프라 | Rust ↔ Ghidra C++ 직접 연동 (업계 유일) |
| 후처리 파이프라인 | PostProcessor 10패스 + CFGStructurizer 12패스 |
| FID 매칭 | 자체 `.fidbf` 파서 + JSON 패턴 매칭 |

---

## 작업 우선순위 (난이도 대비 효과)

| 우선순위 | 항목 | 예상 효과 | 난이도 |
|---------|------|----------|--------|
| 1 | `OptionNullPrinting` + `OptionInPlaceOps` + `OptionHideExtensions` 활성화 | 즉시 출력 품질 향상 | 낮음 (1시간) |
| 2 | Enum 추론 (switch case 분석) | 가독성 향상 | 중간 (1~2일) |
| 3 | Union 복원 | 구조체 정확도 향상 | 중간 (2~3일) |
| 4 | DWARF 파싱 (libdwarf/LLVM) | 디버그 빌드 품질 대폭 향상 | 높음 (1~2주) |
| 5 | 콜그래프 기반 타입 전파 | 전체 분석 정확도 향상 | 높음 (2~4주) |

---

## 관련 문서

- [ROADMAP.md](../ROADMAP.md) — 전체 개발 로드맵
- [TYPE_PROPAGATION_ANALYSIS.md](TYPE_PROPAGATION_ANALYSIS.md) — 타입 전파 세부 분석
- [TYPE_PROPAGATION_STATUS.md](TYPE_PROPAGATION_STATUS.md) — 타입 전파 구현 현황
- [MISSING_FEATURES_ANALYSIS.md](MISSING_FEATURES_ANALYSIS.md) — 이전 기능 격차 분석
- [GCC_FID_IMPLEMENTATION.md](GCC_FID_IMPLEMENTATION.md) — FID 구현 노트
