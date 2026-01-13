# LLVM 기반 디컴파일러 개선 아이디어 (2026-01-13)

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
