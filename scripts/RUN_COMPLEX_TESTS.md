# 🧪 복잡한 테스트 자동 실행 가이드

## 개요

`run_complex_tests.py`는 6개의 복잡한 테스트 케이스를 자동으로 실행하고 Ghidra와 비교하여 결과를 생성하는 스크립트입니다.

## 테스트 케이스

| 번호 | 테스트 | 카테고리 | 난이도 | 설명 |
|------|--------|----------|--------|------|
| 1 | **Nested Loops** | Control Flow | ⭐⭐⭐ | 중첩 루프, break/continue, goto |
| 2 | **Switch-Case** | Control Flow | ⭐⭐ | Fall-through, 중첩 switch |
| 3 | **Recursion** | Control Flow | ⭐⭐⭐⭐ | 단순/다중/상호 재귀 |
| 4 | **Complex Structs** | Data Structures | ⭐⭐⭐⭐ | 중첩 구조체, Union |
| 5 | **Function Pointers** | Pointers | ⭐⭐⭐⭐⭐ | 함수 포인터, 콜백 |
| 6 | **Virtual Functions** | C++ Features | ⭐⭐⭐⭐⭐ | 가상 함수, vtable |

## 사전 준비

### 1. 테스트 바이너리 빌드
```bash
cd test
./build_all_tests.sh
./extract_functions.sh
```

### 2. 필요한 도구 확인
- Python 3.6+
- Ghidra (환경 변수 설정 필요)
- Fission (빌드 완료)

## 실행 방법

### 전체 테스트 실행
```bash
cd /Users/sjkim1127/Fission
python3 scripts/run_complex_tests.py
```

### 출력 예시
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            🧪 Fission Complex Test Suite Runner                
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Output directory: scripts/result_complex_tests_20260108_170000
Total test cases: 6

─────────────────────────────────────────────────────────────────
                          🚀 Running Tests                         
─────────────────────────────────────────────────────────────────

[1/6] [Control Flow] Nested Loops
  Binary: examples/bin_x64/nested_loops_x64.exe
  Difficulty: ⭐⭐⭐
  Functions to test: 56
  Running comparison...
  ✓ Complete
  Similarity: 85.32%
  Duration: 45.2s

[2/6] [Control Flow] Switch-Case
  ...
```

## 생성되는 결과물

### 1. 디렉토리 구조
```
scripts/result_complex_tests_YYYYMMDD_HHMMSS/
├── result_nested_loops/
│   ├── comparison_summary.json
│   ├── addr_0x450_*.txt
│   └── ...
├── result_switch_case/
├── result_recursion/
├── result_complex_structs/
├── result_function_pointers/
├── result_virtual_functions/
├── complex_tests_summary.json     # 전체 요약
└── complex_tests_report.html      # HTML 리포트
```

### 2. JSON 요약 (complex_tests_summary.json)
```json
{
  "timestamp": "2026-01-08T17:00:00",
  "total_tests": 6,
  "success": 6,
  "failed": 0,
  "timeout": 0,
  "average_similarity": 82.45,
  "total_duration": 324.5,
  "tests": [...]
}
```

### 3. HTML 리포트 (complex_tests_report.html)
- 시각적 대시보드
- 카테고리별 결과
- Similarity 색상 코딩
- 클릭 가능한 상세 정보

## 결과 분석

### Similarity 등급
- **90%+**: 🟢 Excellent - Ghidra와 거의 동일
- **80-89%**: 🔵 Good - 약간의 차이
- **70-79%**: 🟡 Fair - 개선 필요
- **<70%**: 🔴 Poor - 많은 개선 필요

### 예상 결과
| 테스트 | 예상 Similarity |
|--------|----------------|
| Switch-Case | 90-95% |
| Nested Loops | 85-90% |
| Recursion | 80-85% |
| Complex Structs | 75-85% |
| Function Pointers | 70-80% |
| Virtual Functions | 65-75% |

## 고급 옵션

### 개별 테스트 실행
스크립트를 수정하여 특정 테스트만 실행:
```python
# run_complex_tests.py 내부
TEST_CASES = [
    # 원하는 테스트만 남기고 주석 처리
    TestCase(...),
]
```

### 타임아웃 조정
```python
# 기본: 600초 (10분)
timeout=600  # 더 복잡한 테스트는 늘리기
```

## 문제 해결

### 1. Ghidra 경로 오류
```bash
export GHIDRA_HOME=/path/to/ghidra_11.4.2_PUBLIC
```

### 2. Fission 빌드 오류
```bash
cd /Users/sjkim1127/Fission
cargo build --release
```

### 3. 주소 파일 없음
```bash
cd test
./extract_functions.sh
```

### 4. 메모리 부족
- 테스트를 개별 실행
- Ghidra 힙 크기 증가

## 성능 최적화

### 병렬 실행 (주의: 메모리 사용량 증가)
현재는 순차 실행이지만, 필요시 수정 가능:
```python
from concurrent.futures import ThreadPoolExecutor
# 병렬 실행 로직 추가
```

### 캐싱
- Ghidra 프로젝트 재사용
- 중간 결과 저장

## 결과 활용

### 1. 개선 우선순위 결정
Similarity가 낮은 카테고리 집중

### 2. 회귀 테스트
코드 변경 후 재실행하여 품질 유지 확인

### 3. 벤치마크
버전별 성능 비교

### 4. 문서화
발견된 한계와 우수 사례 기록

## 추가 정보

- 테스트 소스: `examples/control_flow/`, `examples/data_structures/`, etc.
- 비교 스크립트: `scripts/compare_decompilers_v2.py`
- 빌드 가이드: `examples/README_TESTS.md`

## 문의 및 기여

버그 발견이나 개선 제안은 GitHub Issues로 등록해주세요.
