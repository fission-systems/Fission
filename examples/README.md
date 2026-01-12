# Complex Test Cases for Fission Decompiler

이 디렉토리는 Fission 디컴파일러의 고급 기능을 테스트하기 위한 복잡한 테스트 케이스들을 포함합니다.

## 📁 테스트 카테고리

### 1. Control Flow (`control_flow/`)
복잡한 제어 흐름 패턴 테스트

- **nested_loops.c**: 중첩 루프, break/continue, goto
  - 이중/삼중 중첩 루프
  - Labeled break 시뮬레이션
  - While 안의 for 루프
  - 난이도: ⭐⭐⭐

- **switch_case.c**: 복잡한 switch-case 문
  - Fall-through 케이스
  - 중첩 switch
  - 다중 케이스 라벨
  - 난이도: ⭐⭐

- **recursion.c**: 재귀 함수 패턴
  - 단순 재귀 (factorial)
  - 다중 재귀 호출 (fibonacci)
  - 상호 재귀 (is_even/is_odd)
  - 트리 순회 재귀
  - 난이도: ⭐⭐⭐⭐

### 2. Data Structures (`data_structures/`)
복잡한 데이터 구조 테스트

- **complex_structs.c**: 고급 구조체 패턴
  - 중첩 구조체 (nested structures)
  - 구조체 내 union
  - 함수 포인터를 포함한 구조체
  - 연결 리스트 노드
  - 복잡한 중첩 구조
  - 난이도: ⭐⭐⭐⭐

### 3. Pointers (`pointers/`)
포인터 고급 사용 패턴

- **function_pointers.c**: 함수 포인터
  - typedef를 사용한 함수 포인터
  - 함수 포인터 배열
  - 콜백 패턴
  - 함수 포인터를 반환하는 함수
  - 구조체 멤버로서의 함수 포인터
  - 난이도: ⭐⭐⭐⭐⭐

### 4. C++ Features (`cpp_features/`)
C++ 고유 기능 테스트

- **virtual_functions.cpp**: 가상 함수와 다형성
  - 순수 가상 함수
  - 가상 소멸자
  - 다중 상속
  - 생성자/소멸자에서의 가상 함수 호출
  - 멤버 함수 포인터
  - 난이도: ⭐⭐⭐⭐⭐

## 🔨 빌드 방법

### 전체 빌드
```bash
cd test
./build_all_tests.sh
```

### 개별 빌드 (x86 32-bit)
```bash
# C 파일
x86_64-w64-mingw32-gcc -m32 -O0 -g control_flow/nested_loops.c -o bin_x86/nested_loops_x86.exe

# C++ 파일
x86_64-w64-mingw32-g++ -m32 -O0 -g -std=c++11 cpp_features/virtual_functions.cpp -o bin_x86/virtual_functions_x86.exe
```

### 빌드 옵션 설명
- `-m32`: x86 (32-bit) 타겟
- `-O0`: 최적화 비활성화 (디버그용)
- `-g`: 디버그 심볼 포함
- `-Wall -Wextra`: 모든 경고 활성화
- `-std=c++11`: C++11 표준 사용

## 🧪 테스트 실행

### 개별 실행
```bash
cd test/bin_x86
./nested_loops_x86.exe
./function_pointers_x86.exe
```

### Fission으로 디컴파일
```bash
# 단일 함수
fission bin_x86/nested_loops_x86.exe --decompile 0x401000

# 모든 함수
fission bin_x86/nested_loops_x86.exe --functions

# Ghidra와 비교
python3 scripts/compare_decompilers_v2.py \
    test/bin_x86/nested_loops_x86.exe \
    test/nested_loops_addresses.txt \
    scripts/result_nested_loops \
    --batch
```

## 📊 예상 테스트 결과

### 난이도별 예상 Similarity

| 카테고리 | 난이도 | 예상 Similarity | 비고 |
|----------|--------|----------------|------|
| **단순 제어 흐름** | ⭐⭐ | 95-100% | 이미 검증됨 |
| **중첩 루프** | ⭐⭐⭐ | 90-95% | Goto 처리 필요 |
| **재귀** | ⭐⭐⭐⭐ | 85-95% | 복잡한 재귀는 도전적 |
| **복잡한 구조체** | ⭐⭐⭐⭐ | 80-90% | 구조체 복구 정확도 |
| **함수 포인터** | ⭐⭐⭐⭐⭐ | 70-85% | 타입 추론 어려움 |
| **C++ 가상 함수** | ⭐⭐⭐⭐⭐ | 60-80% | vtable 처리 필요 |

### 테스트 목표

1. **기능 검증**: 복잡한 패턴에서도 작동 확인
2. **한계 발견**: 개선이 필요한 영역 식별
3. **품질 측정**: 실제 코드에서의 성능 평가
4. **비교 분석**: Ghidra와의 상세 비교

## 🐛 알려진 문제

현재 Fission의 예상 약점:

1. **함수 포인터 타입**: `void*`로 추론될 가능성
2. **C++ vtable**: vtable 포인터를 명시적으로 표시 안 할 수 있음
3. **복잡한 재귀**: 최적화된 tail recursion 인식 어려움
4. **Union 타입**: Union 멤버 구분 부정확할 수 있음
5. **다중 상속**: 부모 클래스 구분 어려움

## 📝 결과 분석

각 테스트 실행 후:

1. **함수 목록 확인**
   ```bash
   fission bin_x86/test.exe --functions > functions.txt
   ```

2. **주요 함수 디컴파일**
   - Entry point
   - 가장 복잡한 함수
   - 재귀 함수
   - 가상 함수

3. **Ghidra와 비교**
   - Similarity 점수
   - 차이점 분석
   - 개선 방향 도출

4. **문서화**
   - 발견된 문제점
   - 우수한 처리 사례
   - 개선 제안

## 🎯 성공 기준

### 최소 목표 (Pass)
- 모든 테스트 컴파일 성공
- 모든 바이너리에서 함수 인식
- 단순 함수 90%+ similarity

### 목표 (Good)
- 중간 복잡도 함수 85%+ similarity
- 구조체 필드 60%+ 정확도
- 함수 포인터 인식

### 우수 목표 (Excellent)
- 복잡한 함수 80%+ similarity
- C++ 가상 함수 테이블 인식
- 재귀 패턴 완벽 복원

## 🔄 지속적 개선

이 테스트 스위트는:
1. 새로운 개선 사항 검증
2. 회귀 테스트 (regression test)
3. 벤치마크 기준선
4. 개발 방향 가이드

로 활용됩니다.

## 📚 참고 자료

- `docs/analysis/IMPROVEMENT_LOG.md`: 개선 기록
- `docs/analysis/FUTURE_IMPROVEMENTS.md`: 개선 계획
- `docs/decompiler/DECOMPILER_COMPARISON.md`: 비교 분석
- `scripts/compare_decompilers_v2.py`: 비교 스크립트
