# 🧪 Fission 테스트 아키텍처

## 현재 시스템 구조

### ✅ PyGhidra 사용 중!

현재 Fission은 이미 **pyghidra 2.2.1**을 사용하여 Ghidra를 자동화하고 있습니다.

```
테스트 흐름:
┌─────────────────────────────────────────────────────────┐
│  run_complex_tests.py (자동화 러너)                     │
│  ├─ 6개 테스트 케이스 순회                              │
│  └─ 각 테스트마다 compare_decompilers_v2.py 호출        │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  compare_decompilers_v2.py (비교 엔진)                  │
│  ├─ Ghidra: pyghidra_decompile.py 호출 ✅               │
│  ├─ Fission: fission_cli --decomp 호출                  │
│  └─ 결과 비교 및 similarity 계산                        │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  pyghidra_decompile.py (Ghidra 래퍼)                    │
│  └─ pyghidra.open_program() 사용 ✅                     │
│     ├─ 바이너리 로드 및 자동 분석                       │
│     ├─ 함수 디스어셈블리 추출                           │
│     └─ 디컴파일 결과 생성                               │
└─────────────────────────────────────────────────────────┘
```

## PyGhidra의 장점

### 1. **자동화 친화적**
```python
with pyghidra.open_program(binary_path, analyze=True) as flat_api:
    program = flat_api.getCurrentProgram()
    # Ghidra API 직접 사용 가능
```

### 2. **GUI 불필요**
- Headless 모드로 실행
- CI/CD 파이프라인 통합 가능
- 서버 환경에서도 실행 가능

### 3. **빠른 실행**
- Ghidra GUI 로딩 없음
- Python에서 직접 제어
- 프로젝트 생성/관리 자동화

### 4. **안정성**
- Java 프로세스 격리
- 타임아웃 제어
- 에러 핸들링 용이

## 현재 구현 상세

### 1. pyghidra_decompile.py
```python
# 위치: scripts/ghidra/pyghidra_decompile.py

주요 기능:
✅ 바이너리 로드 (PE/ELF 지원)
✅ 자동 분석 (analyze=True)
✅ 함수 디스어셈블리 추출
✅ 디컴파일 수행
✅ 결과 포맷팅

출력 형식:
- Assembly Listing (주소, 니모닉, 오퍼랜드)
- Decompiled Code (C-like 코드)
- 함수 메타데이터
```

### 2. compare_decompilers_v2.py
```python
# 위치: scripts/compare/compare_decompilers_v2.py

주요 기능:
✅ Ghidra + Fission 병렬 실행
✅ 출력 정규화 (ANSI 제거, 노이즈 필터링)
✅ Similarity 계산 (difflib 사용)
✅ 타이밍 측정
✅ JSON/HTML 리포트 생성

비교 메트릭:
- Line-by-line similarity
- 코드 구조 분석
- 성능 비교 (실행 시간)
```

### 3. run_complex_tests.py
```python
# 위치: scripts/run_complex_tests.py

주요 기능:
✅ 6개 복잡한 테스트 자동 실행
✅ 카테고리별 결과 정리
✅ 통계 분석 (난이도별, 카테고리별)
✅ HTML 리포트 생성
✅ 진행 상황 실시간 표시

테스트 대상:
1. Control Flow (3개)
2. Data Structures (1개)
3. Pointers (1개)
4. C++ Features (1개)
```

## PyGhidra 설정

### 현재 환경
```bash
PyGhidra Version: 2.2.1
Ghidra Path: /Users/sjkim1127/Fission/ghidra_11.4.2_PUBLIC
Python: 3.x
```

### 설정 확인
```bash
# PyGhidra 설치 확인
python3 -c "import pyghidra; print(pyghidra.__version__)"

# Ghidra 경로 확인
echo $GHIDRA_INSTALL_DIR

# 또는 스크립트에서 자동 설정
# scripts/ghidra/pyghidra_decompile.py:15-16
```

## 성능 최적화

### 1. 분석 캐싱
PyGhidra는 자동으로 Ghidra 프로젝트를 생성하고 캐싱합니다:
```
~/.local/share/pyghidra/projects/
├── project_ABC123/
│   ├── project.gpr
│   ├── project.rep/
│   └── ...
```

### 2. 병렬 실행 가능성
현재는 순차 실행이지만, 개선 가능:
```python
from concurrent.futures import ProcessPoolExecutor

# 여러 바이너리를 병렬로 처리
with ProcessPoolExecutor(max_workers=4) as executor:
    futures = [executor.submit(run_test, test) for test in tests]
```

⚠️ **주의**: Ghidra는 메모리 사용량이 많으므로 동시 실행 수 제한 필요

### 3. 타임아웃 설정
```python
# compare_decompilers_v2.py
timeout=600  # 10분

# 큰 바이너리는 더 긴 타임아웃 필요
timeout=1200  # 20분
```

## 테스트 실행 예제

### 간단한 테스트
```bash
# 단일 함수 디컴파일 (PyGhidra 사용)
python3 scripts/ghidra/pyghidra_decompile.py \
    examples/bin_x64/nested_loops_x64.exe \
    0x450
```

### 비교 테스트
```bash
# Ghidra vs Fission 비교
python3 scripts/compare_decompilers_v2.py \
    examples/bin_x64/nested_loops_x64.exe \
    examples/addresses/nested_loops_addrs.txt \
    scripts/result_nested_loops \
    --batch
```

### 전체 테스트 스위트
```bash
# 모든 복잡한 테스트 실행 (PyGhidra 사용)
python3 scripts/run_complex_tests.py
```

## 문제 해결

### 1. PyGhidra 설치 오류
```bash
# 재설치
pip3 uninstall pyghidra
pip3 install pyghidra

# 또는 개발 버전
pip3 install git+https://github.com/Defense-Cyber-Crime-Center/pyghidra.git
```

### 2. Ghidra 경로 문제
```bash
# 환경 변수 설정
export GHIDRA_INSTALL_DIR=/path/to/ghidra_11.4.2_PUBLIC

# 또는 스크립트 수정
# scripts/ghidra/pyghidra_decompile.py:15
ghidra_path = "/custom/path/to/ghidra"
```

### 3. 메모리 부족
```bash
# Ghidra JVM 힙 크기 조정
export _JAVA_OPTIONS="-Xmx8G"

# 또는 동시 실행 수 줄이기
max_workers=2  # 병렬 실행 시
```

### 4. Java 버전 문제
```bash
# Ghidra 11.4.2는 Java 17+ 필요
java -version

# Java 설정
export JAVA_HOME=/path/to/java17
```

## 향후 개선 방향

### 1. ✅ 이미 구현됨
- [x] PyGhidra 통합
- [x] 자동화된 테스트 실행
- [x] Similarity 계산
- [x] HTML 리포트 생성

### 2. 🔄 개선 가능
- [ ] 병렬 실행 (메모리 관리 필요)
- [ ] 분석 결과 캐싱 최적화
- [ ] 타임아웃 동적 조정
- [ ] 실패한 테스트 재시도 로직

### 3. 🆕 새로운 기능
- [ ] 실시간 대시보드
- [ ] 히스토리 추적 (버전별 비교)
- [ ] CI/CD 통합
- [ ] Slack/이메일 알림

## 참고 자료

- **PyGhidra 문서**: https://github.com/Defense-Cyber-Crime-Center/pyghidra
- **Ghidra API**: https://ghidra.re/ghidra_docs/api/
- **프로젝트 구조**: `scripts/README.md`
- **테스트 가이드**: `examples/README_TESTS.md`

## 요약

✅ **PyGhidra를 이미 사용하고 있습니다!**

현재 시스템은:
- PyGhidra로 Ghidra 자동화
- 완전한 헤드리스 모드 실행
- 빠르고 안정적인 테스트 파이프라인
- 포괄적인 결과 분석 및 리포팅

추가 작업 없이 바로 `python3 scripts/run_complex_tests.py`로 테스트 시작 가능합니다! 🚀
