# LLVM-Based Decompiler Improvement Ideas (2026-01-13)

> ⚠️ **Status: Exploratory, Not Active Roadmap**
> This document captures speculative LLVM-inspired ideas.
> It is useful as research context, not as an active implementation plan.

LLVM 프로젝트의 최신 최적화 패스 및 분석 기법을 역으로 적용하여 Fission 디컴파일러의 품질을 혁신적으로 향상시키기 위한 아이디어 리스트입니다.

## 1. SCEV (Scalar Evolution)를 이용한 루프 분석 고도화

- **LLVM 참고**: `llvm/Analysis/ScalarEvolution.h`
- **목표**: 루프의 반복 횟수(Trip Count)와 인덕션 변수(Induction Variable)를 수학적으로 정밀하게 추론.
- **세부 내용**:
  - `i = i + 2`, `i = i * 2 + 1` 과 같은 복잡한 증감 패턴을 일반식으로 표현.
  - 다중 중첩 루프에서 배열 인덱스 접근 식(`arr[i*n + j]`)을 다차원 배열 형태(`arr[i][j]`)로 복구하는 기반 제공.

## 2. Loop Idiom Recognition 역추적

- **LLVM 참고**: `llvm/Transforms/Scalar/LoopIdiomRecognize.cpp`
- **목표**: 컴파일러에 의해 인라인되거나 최적화된 표준 함수 패턴 복구.
- **세부 내용**:
  - 루프 형태로 구현된 `memset`, `memcpy`, `memmove`, `strlen` 등을 탐지하여 고수준 함수 호출로 치환.
  - 최신 CPU 명령어(AVX-512, NEON 등)를 사용한 벡터화된 루프를 원래의 단순 루프나 표준 함수로 복구.

## 3. SROA (Scalar Replacement of Aggregates) 역변환

- **LLVM 참고**: `llvm/Transforms/Scalar/SROA.cpp`
- **목표**: 여러 개의 스칼라 변수로 쪼개진 구조체 필드들을 하나의 구조체 변수로 재결합.
- **세부 내용**:
  - 컴파일러가 최적화를 위해 `struct Point { int x, y; }`를 `local_x`, `local_y`로 쪼개 놓은 것을 다시 `struct` 접근 형태로 복구.
  - 스택 레이아웃 분석과 연동하여 필드 간의 오프셋 관계를 기반으로 구조체 타입 추론.

## 4. DFA (Deterministic Finite Automaton) 기반 제어 흐름 분석

- **LLVM 참고**: `llvm/Transforms/Scalar/DFAJumpThreading.cpp`
- **목표**: 복잡한 상태 머신(State Machine) 형태의 난독화 코드 해독.
- **세부 내용**:
  - `switch` 문과 `goto`를 이용한 비정형 제어 흐름(예: OLLVM의 Control Flow Flattening)을 분석하여 원래의 `if-else` 또는 루프 구조로 복구.
  - 상태 변수의 값 변화를 추적하여 도달 불가능한 경로(Dead Code) 제거 성능 향상.

## 5. Alias Analysis를 이용한 포인터 추론 개선

- **LLVM 참고**: `llvm/Analysis/AliasAnalysis.h`
- **목표**: 포인터가 가리키는 대상이 같은지(Must-Alias) 다른지(No-Alias) 명확히 판별하여 변수 전파 성능 향상.
- **세부 내용**:
  - `CFL-AA` (Context-Free Language Alias Analysis) 기법을 도입하여 함수 호출 경계를 넘나드는 포인터 흐름 추적.
  - `MemorySSU`를 활용하여 메모리 쓰기 동작이 다른 변수에 미치는 영향을 정밀하게 분석함으로써 잘못된 변수 병합 방지.

## 6. Constraint Elimination 기반 조건문 단순화

- **LLVM 참고**: `llvm/Transforms/Scalar/ConstraintElimination.cpp`
- **목표**: 수학적 제약 조건 분석을 통한 중복 조건문 및 난독화된 술어(Predicate) 제거.
- **세부 내용**:
  - `if (x < 10) { if (x < 20) ... }`과 같은 중복 조건 제거.
  - 불투명 술어(Opaque Predicate)와 같이 항상 참/거짓인 수식을 계산하여 제어 흐름 단순화.

## 7. Polyhedral Optimization (Polly) 기반 고차원 분석

- **LLVM 참고**: `vendor/llvm-project-main/polly`
- **목표**: 극도로 복잡하게 최적화된 행렬 연산 및 다차원 배열 루프 복구.
- **세부 내용**:
  - 루프 타일링(Tiling), 스와핑(Interchange) 등이 적용된 수치 연산 코드를 원래의 알고리즘 수식에 가깝게 복원.

## 8. LLVM Attributor를 이용한 함수 속성 정밀 추론

- **LLVM 참고**: `llvm/Transforms/IPO/Attributor.cpp`
- **목표**: 인터프로시저럴(Interprocedural) 분석을 통해 함수의 부수 효과(Side Effects) 및 인자 특성 파악.
- **세부 내용**:
  - `readonly`, `readnone`, `nonnull`, `noalias` 등의 속성을 추론하여 디컴파일 결과에 `const` 키워드나 최적화된 포인터 타입을 자동 적용.
  - 함수 호출 그래프 전역에서 데이터 흐름을 추적하여 특정 전역 변수가 어디서 수정되는지 명확히 시각화.

## 9. Delinearization (다차원 배열 복구)

- **LLVM 참고**: `llvm/Analysis/Delinearization.cpp`
- **목표**: 1차원으로 펼쳐진 메모리 접근 식(`base + i * size1 + j * size2`)을 다차원 배열 접근(`arr[i][j]`)으로 변환.
- **세부 내용**:
  - SCEV 분석 결과와 연동하여 다차원 배열의 각 차원 크기(Stride)를 수학적으로 분리.
  - 구조체 내부의 중첩 배열(Nested Array) 구조를 원형에 가깝게 복원.

## 10. Stack Lifetime 분석을 통한 변수 스코프 복구

- **LLVM 참고**: `llvm/Analysis/StackLifetime.cpp`
- **목표**: 스택 슬롯 재사용 패턴을 탐지하여 서로 다른 논리적 변수를 분리.
- **세부 내용**:
  - 컴파일러가 최적화를 위해 서로 다른 블록에 있는 변수들을 동일한 스택 위치에 할당한 경우, 이를 분석하여 별개 변수로 디컴파일.
  - 로컬 변수의 실제 "생존 기간(Lifetime)"을 계산하여 중괄호(`{}`)를 이용한 변수 스코프 최소화.

## 11. Demanded Bits 분석을 통한 산술 식 최적화

- **LLVM 참고**: `llvm/Analysis/DemandedBits.cpp`
- **목표**: 비트 마스킹 및 쉬프트 연산이 반복되는 난독화된 산술 식 단순화.
- **세부 내용**:
  - 실제로 최종 결과에 영향을 미치는 비트만 추적하여, 중간 단계의 불필요한 `and`, `or`, `shl` 연산 제거.
  - 플래그 계산 로직이나 비트 필드(Bit-field) 접근 코드를 표준 C 문법으로 정제.

## 12. Capture Tracking을 이용한 포인터 탈출 분석

- **LLVM 참고**: `llvm/Analysis/CaptureTracking.cpp`
- **목표**: 포인터가 함수 외부(전역 변수, 다른 함수 인자 등)로 노출되는지 확인하여 타입 안전성 검증.
- **세부 내용**:
  - 지역 변수의 포인터가 함수 외부로 전달되지 않음을 보장(Capture되지 않음)함으로써, 해당 변수의 타입을 더 안전하게 확정.
  - `this` 포인터나 객체 인스턴스의 생명 주기를 더 정확하게 추론.

## 13. IR Similarity Identifier 기반 라이브러리 코드 탐지

- **LLVM 참고**: `llvm/Analysis/IRSimilarityIdentifier.cpp`
- **목표**: 구조적으로 유사한 P-code 블록을 찾아내어 라이브러리 코드나 중복 로직 식별.
- **세부 내용**:
  - 바이너리 내에 인라인된 동일한 라이브러리 함수들을 찾아내어 공통 심볼 부여.
  - 코드 복사-붙여넣기로 생성된 패턴을 분석하여 고수준 매크로나 템플릿 구조 추론.

## 14. Whole-Program Devirtualization 역추적

- **LLVM 참고**: `llvm/Transforms/IPO/WholeProgramDevirt.cpp`
- **목표**: 컴파일 타임에 최적화된 가상 함수 호출(Virtual Call)을 원래의 가상 함수 테이블(VTable) 호출 구조로 복원.
- **세부 내용**:
  - 클래스 계층 구조를 추측하여, 직접 호출로 바뀐 코드를 가상 함수 호출 형태로 디컴파일하여 객체지향 구조 명확화.
