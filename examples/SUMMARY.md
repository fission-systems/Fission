# 복잡한 테스트 케이스 요약

## 🎯 빌드 완료

모든 테스트 케이스가 성공적으로 빌드되었습니다 (x86_64, MinGW):

| 테스트 파일 | 바이너리 | 함수 개수 | 난이도 |
|------------|---------|----------|--------|
| `nested_loops.c` | `nested_loops_x64.exe` | 56 | ⭐⭐⭐ |
| `switch_case.c` | `switch_case_x64.exe` | 55 | ⭐⭐ |
| `recursion.c` | `recursion_x64.exe` | 60 | ⭐⭐⭐⭐ |
| `complex_structs.c` | `complex_structs_x64.exe` | 63 | ⭐⭐⭐⭐ |
| `function_pointers.c` | `function_pointers_x64.exe` | 77 | ⭐⭐⭐⭐⭐ |
| `virtual_functions.cpp` | `virtual_functions_x64.exe` | 145 | ⭐⭐⭐⭐⭐ |

## 📂 디렉토리 구조

```
test/
├── control_flow/           # 제어 흐름 테스트
│   ├── nested_loops.c
│   ├── switch_case.c
│   └── recursion.c
├── data_structures/        # 데이터 구조 테스트
│   └── complex_structs.c
├── pointers/              # 포인터 테스트
│   └── function_pointers.c
├── cpp_features/          # C++ 기능 테스트
│   └── virtual_functions.cpp
├── bin_x64/               # 빌드된 바이너리
│   └── *.exe
├── addresses/             # 추출된 함수 주소
│   └── *_addrs.txt
├── build_all_tests.sh     # 전체 빌드 스크립트
├── extract_functions.sh   # 함수 주소 추출 스크립트
└── run_tests.sh          # 테스트 실행 스크립트
```

## 🚀 사용 방법

### 1. 빌드
```bash
cd test
./build_all_tests.sh
```

### 2. 함수 주소 추출
```bash
./extract_functions.sh
```

### 3. Fission으로 디컴파일
```bash
# 단일 함수
fission bin_x64/nested_loops_x64.exe --decompile 0x450

# 전체 비교 (Ghidra vs Fission)
python3 ../scripts/compare_decompilers_v2.py \
    bin_x64/nested_loops_x64.exe \
    addresses/nested_loops_addrs.txt \
    ../scripts/result_nested_loops \
    --batch
```

## 📊 테스트 카테고리별 상세

### 1️⃣ Control Flow (제어 흐름)

**nested_loops.c** - 중첩 루프
- `find_pair()`: 이중 루프 + early return
- `print_3d_matrix()`: 삼중 중첩 루프 + continue
- `find_in_matrix()`: goto를 사용한 labeled break
- `complex_iteration()`: while 안의 for 루프

**switch_case.c** - Switch-Case
- `get_day_type()`: Fall-through 케이스
- `calculate_score()`: 중첩 switch
- `process_command()`: 16진수 case 값
- `parse_simple_command()`: Switch + 문자열 체크

**recursion.c** - 재귀
- `factorial()`: 단순 재귀
- `fibonacci()`: 이중 재귀 호출
- `is_even()`/`is_odd()`: 상호 재귀
- `ackermann()`: 복잡한 다중 재귀
- `sum_tree()`: 트리 순회

### 2️⃣ Data Structures (데이터 구조)

**complex_structs.c**
- 중첩 구조체 (`Point3D`, `Line3D`, `Player`)
- Union을 포함한 구조체 (`Variant`)
- 함수 포인터를 가진 구조체 (`DynamicArray`)
- 이중 연결 리스트 (`ListNode`)
- 복잡한 중첩 레코드 (`ComplexRecord`)

### 3️⃣ Pointers (포인터)

**function_pointers.c**
- `typedef`를 사용한 함수 포인터 (`BinaryOp`)
- 함수 포인터 배열
- 구조체 멤버로서의 함수 포인터 (`FilterFunc`)
- 콜백 패턴 (`EventCallback`)
- 함수 포인터를 반환하는 함수 (`get_math_function`)
- 함수 포인터의 포인터 (`CompareFuncGetter`)

### 4️⃣ C++ Features (C++ 기능)

**virtual_functions.cpp**
- 순수 가상 함수 (`Shape::area()`)
- 가상 소멸자
- 다중 상속 (`Document : Printable, Serializable`)
- 생성자/소멸자에서의 가상 함수 호출
- 멤버 함수 포인터 (`Calculator::Operation`)
- vtable 생성

## 🎯 예상 테스트 결과

### 디컴파일 품질 예측

| 테스트 | 로컬 변수 | 타입 추론 | 제어 흐름 | 전체 예상 |
|--------|---------|---------|----------|----------|
| **nested_loops** | 95% | 90% | 90% | **92%** |
| **switch_case** | 95% | 95% | 95% | **95%** |
| **recursion** | 90% | 85% | 90% | **88%** |
| **complex_structs** | 85% | 75% | 95% | **85%** |
| **function_pointers** | 80% | 60% | 95% | **78%** |
| **virtual_functions** | 75% | 50% | 90% | **72%** |

### 예상되는 강점

1. ✅ **제어 흐름 복원**: 중첩 루프, switch-case 우수
2. ✅ **로컬 변수 분리**: 이전 개선으로 해결됨
3. ✅ **상수 표현**: 부동소수점, 문자열 인라인 개선됨
4. ✅ **단순 구조체**: 중첩 구조체도 잘 처리됨

### 예상되는 약점

1. ⚠️ **함수 포인터 타입**: `void*`로 추론될 가능성
2. ⚠️ **C++ vtable**: vtable 구조 명시적 표시 어려움
3. ⚠️ **복잡한 Union**: Union 멤버 구분 부정확
4. ⚠️ **다중 상속**: 부모 클래스 오프셋 계산

## 📈 벤치마크 계획

### Phase 1: 기본 검증
```bash
# 각 테스트에서 main 함수 디컴파일
for exe in bin_x64/*.exe; do
    fission "$exe" --functions | grep main
done
```

### Phase 2: Ghidra 비교
```bash
# Control Flow 테스트
python3 ../scripts/compare_decompilers_v2.py \
    bin_x64/nested_loops_x64.exe \
    addresses/nested_loops_addrs.txt \
    ../scripts/result_nested_loops --batch

# 각 카테고리 실행...
```

### Phase 3: 결과 분석
- Similarity 점수 수집
- 차이점 분류 (타입 vs 구조 vs 변수명)
- 개선 우선순위 도출

## 🐛 디버깅

### 함수 확인
```bash
# 특정 바이너리의 함수 목록
x86_64-w64-mingw32-objdump -t bin_x64/nested_loops_x64.exe | grep "(ty   20)"

# 함수 디스어셈블리
x86_64-w64-mingw32-objdump -d bin_x64/nested_loops_x64.exe | less
```

### 주소 확인
```bash
# 추출된 주소 목록
cat addresses/nested_loops_addrs.txt
```

## 📝 다음 단계

1. ✅ **빌드 완료** - 모든 테스트 컴파일됨
2. ✅ **함수 추출** - 주소 목록 생성됨
3. ⏳ **Fission 테스트** - 각 바이너리 디컴파일
4. ⏳ **Ghidra 비교** - 비교 벤치마크 실행
5. ⏳ **결과 분석** - 개선점 도출

## 🎓 학습 포인트

이 테스트 스위트는 다음을 검증합니다:

1. **제어 흐름 복원 정확도**
   - 중첩 루프, goto, switch-case
   
2. **타입 추론 능력**
   - 구조체 필드, 함수 포인터, vtable
   
3. **코드 가독성**
   - 변수명, 타입명, 포맷팅
   
4. **엣지 케이스 처리**
   - 재귀, 상호 재귀, 복잡한 포인터

각 실패는 개선 기회입니다! 🚀
