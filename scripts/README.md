# Fission Scripts

이 폴더에는 Fission 개발 및 비교 분석을 위한 유틸리티 스크립트들이 포함되어 있습니다.

## 📋 목차

- [스크립트 목록](#스크립트-목록)
- [설치 요구사항](#설치-요구사항)
- [사용법](#사용법)
  - [Decompiler 빌드](#1-decompiler-빌드)
  - [디컴파일러 비교](#2-디컴파일러-비교)
  - [PyGhidra 디컴파일](#3-pyghidra-디컴파일)
  - [Ghidra 디컴파일](#4-ghidra-디컴파일)
  - [FID 데이터베이스 테스트](#5-fid-데이터베이스-테스트)
- [출력 형식](#출력-형식)
- [문제 해결](#문제-해결)

---

## 스크립트 목록

| 스크립트 | 설명 | 사용 목적 |
|---------|------|----------|
| `build/build_decompiler.sh` | Ghidra 디컴파일러 C++ 컴포넌트 빌드 | 네이티브 디컴파일 기능 활성화 |
| `build/build_all_tests.sh` | x86 테스트 바이너리 빌드 (MinGW) | 테스트 케이스 준비 |
| `build/dev-tools.sh` | 프로파일링·퍼징·ASAN 등 개발 도구 | 개발/디버그용 |
| `compare/compare_decompilers.sh` | Ghidra vs Fission 단일 함수 비교 | 성능·품질 비교 |
| `compare/compare_decompilers_v2.sh` | 단일 함수 비교 + 텍스트 추출 | 원문 텍스트 파일 생성 |
| `compare/compare_decompilers_v2.py` | v2 Python 구현 (v2 쉘에서 호출) | 배치·HTML 리포트 |
| `compare/extract_functions.sh` | 테스트 바이너리에서 함수 주소 추출 | 비교 준비 |
| `compare/fix_addresses.py` | RVA에 ImageBase 가산 | 주소 변환 |
| `ghidra/pyghidra_decompile.py` | PyGhidra를 이용한 함수 디컴파일 | Python 환경에서 Ghidra 활용 |
| `ghidra/pyghidra_decompile_batch.py` | PyGhidra 배치 디컴파일 | 다수 함수 일괄 처리 |
| `ghidra/convert_fidb_batch.sh` | FIDB → FIDf 배치 변환 | 함수 식별 DB 변환 |
| `test/test_fid.sh` | FID 데이터베이스 로딩 테스트 | 함수 식별 데이터베이스 검증 |
| `test/run_tests.sh` | 테스트 바이너리 실행 검증 | 빌드 아티팩트 동작 확인 |
| `test/run_complex_tests.py` | 복합 디컴파일 테스트 자동화 | 회귀 테스트 및 HTML 리포트 |
| `test/test_cfg_structurizer.py` | CFG 구조화 알고리즘 단위 테스트 | CFG 분석 검증 |
| `test/test_postprocessor.py` | 후처리기 단위 테스트 | 디컴파일 후처리 검증 |
| `lint/cppcheck.sh` | Cppcheck (C++) | 코어/FFI/디컴파일 파이프라인 점검 |

> 📄 **상세 문서**
> - 복합 테스트 실행: [`docs/RUN_COMPLEX_TESTS.md`](../docs/RUN_COMPLEX_TESTS.md)
> - 테스트 아키텍처: [`docs/TESTING_ARCHITECTURE.md`](../docs/TESTING_ARCHITECTURE.md)

---

## Directory layout

```
scripts/
├── build/           # 빌드 헬퍼 (decompiler 빌드, 테스트 바이너리 빌드, dev-tools)
├── compare/         # 비교 스크립트 및 주소 데이터
├── ghidra/          # Ghidra / PyGhidra 헬퍼
├── lint/            # 린터 (cppcheck 등)
└── test/            # 테스트 헬퍼 (unit tests, 실행 검증, 복합 테스트)
```

> 결과 파일은 `scripts/` 내부가 아닌 `examples/results/` 또는 로컬 `scripts/result/`(gitignore됨)에 저장됩니다.

---

## utils/ 와 크레이트 참조

`utils/` 는 서명·타입 정보 등 공용 데이터 디렉터리입니다.

```
utils/
└── signatures/           # DIE 규칙, FID 데이터베이스, Windows API 서명 등
    ├── die/              # Detect-It-Easy 엔진 스크립트
    └── ...
```

일부 크레이트(`fission-loader`, `fission-signatures` 등)가 빌드 시 저장소 루트 기준
상대 경로(`../../utils/signatures/die/…`)로 `utils/` 를 참조합니다.

> ⚠️ **항상 저장소 루트에서 `cargo build` / `cargo test` 를 실행**하세요.
> 하위 디렉터리에서 빌드하면 위 상대 경로가 깨져 컴파일·런타임 오류가 발생합니다.

---

## 설치 요구사항

### 공통 요구사항

```bash
# Rust 툴체인
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Fission 빌드
cargo build --release
```

### 스크립트별 추가 요구사항

#### build/build_decompiler.sh

```bash
# macOS
brew install cmake ninja

# Linux (Ubuntu/Debian)
sudo apt-get install cmake build-essential
```

#### compare/compare_decompilers.sh & ghidra/pyghidra_decompile.py

```bash
# Python 3.8+ 필요
pip3 install pyghidra

# jq (JSON 파싱, 선택사항)
brew install jq  # macOS
sudo apt-get install jq  # Linux
```

#### ghidra/pyghidra_decompile.py

- Ghidra 11.4.2+ 설치 필요
- `GHIDRA_INSTALL_DIR` 환경 변수 설정

---

## 사용법

### 1. Decompiler 빌드

Ghidra 디컴파일러 C++ 컴포넌트를 빌드하여 네이티브 디컴파일 기능을 활성화합니다.

```bash
# 기본 사용
./scripts/build/build_decompiler.sh

# 실행 과정
# [*] Configuring with CMake...
# [*] Compiling...
# [*] Build complete!
```

**출력 위치:** `ghidra_decompiler/build/libdecompiler.a` 또는 `.so/.dylib`

**주의사항:**

- CMake 3.10+ 필요
- C++17 지원 컴파일러 필요
- 빌드 시간: 약 1-3분

---

### 2. 디컴파일러 비교

Ghidra와 Fission의 디컴파일 결과를 비교하고 JSON 형식으로 출력합니다.

#### A. 단일 함수 비교 + 텍스트 추출 (v2 - 권장)

```bash
./scripts/compare/compare_decompilers_v2.sh <binary> <address> [output.json]

# 예제 - 자동으로 타임스탬프 폴더 생성
./scripts/compare/compare_decompilers_v2.sh test.exe 0x140001450

# 출력 경로를 직접 지정할 수도 있음
./scripts/compare/compare_decompilers_v2.sh test.exe 0x401000 results/custom.json
```

**자동 폴더 생성:**

- 출력 경로를 지정하지 않으면 `scripts/result/{타임스탬프}_result/` 폴더가 자동 생성됩니다
- 타임스탬프 형식: `YYYYMMDDHHMM` (예: `202601061104`)
- 매 실행마다 새로운 폴더가 생성되어 결과가 보존됩니다

**폴더 구조 예시:**

```
scripts/result/
├── 202601061104_result/
│   ├── comparison.json
│   ├── comparison_ghidra_asm.txt
│   ├── comparison_ghidra_decomp.txt
│   ├── comparison_fission_asm.txt
│   └── comparison_fission_decomp.txt
└── 202601061106_result/
    ├── comparison.json
    ├── comparison_ghidra_asm.txt
    ├── comparison_ghidra_decomp.txt
    ├── comparison_fission_asm.txt
    └── comparison_fission_decomp.txt
```

**생성되는 파일:**

```
{타임스탬프}_result/comparison.json              # JSON 통합 데이터
{타임스탬프}_result/comparison_ghidra_asm.txt    # Ghidra 어셈블리 (원문)
{타임스탬프}_result/comparison_ghidra_decomp.txt # Ghidra 디컴파일 (원문)
{타임스탬프}_result/comparison_fission_asm.txt   # Fission 어셈블리 (원문)
{타임스탬프}_result/comparison_fission_decomp.txt # Fission 디컴파일 (원문)
```

**특징:**

- ✅ 함수 경계 자동 감지 (RET 명령어까지)
- ✅ Ghidra 어셈블리와 디컴파일을 자동 분리
- ✅ 4개의 독립적인 텍스트 파일 생성
- ✅ ANSI 코드 자동 제거
- ✅ 원문 그대로 추출 (비교/분석에 용이)
- ✅ Ghidra/Fission 디컴파일 시간 측정 (timings)
- ✅ Fission `--disasm-function` 옵션 사용

> **💡 중요:** Fission은 `--disasm-function` 옵션을 사용하여 함수 경계를 자동으로 감지합니다.  
> 함수 크기 정보가 없는 경우 RET 명령어를 찾아 자동으로 경계를 결정하므로,  
> Ghidra와 정확히 동일한 명령어 수가 출력됩니다. (고정 개수가 아님)

**사용 예시:**

```bash
# 실행
./scripts/compare/compare_decompilers_v2.sh test.exe 0x140001450 results/add.json

# Ghidra 디컴파일 보기
cat results/add_ghidra_decomp.txt

# Fission vs Ghidra 디컴파일 비교
diff results/add_ghidra_decomp.txt results/add_fission_decomp.txt

# 어셈블리 비교
diff results/add_ghidra_asm.txt results/add_fission_asm.txt
```

#### B. 배치 모드 + HTML 리포트 (v2)

```bash
./scripts/compare/compare_decompilers_v2.sh -m <binary> <address_file> <output_dir>

# HTML 리포트 포함
./scripts/compare/compare_decompilers_v2.sh -m -h test.exe addresses.txt results/

# 타임아웃 설정 (기본: 600초)
./scripts/compare/compare_decompilers_v2.sh -m -h -t 600 test.exe addresses.txt results/
```

**추가 출력 (배치 모드):**

- `summary.json` (타이밍 평균/중앙값/최소/최대, 승자 카운트)

#### C. Legacy (v1)

```bash
./scripts/compare/compare_decompilers.sh <binary> <address> [output_json]

# 예제
./scripts/compare/compare_decompilers.sh examples/comparison_test_x64.exe 0x140001450
./scripts/compare/compare_decompilers.sh my_program 0x401000 results/comparison.json
```

#### 배치 모드 (v1)

```bash
./scripts/compare/compare_decompilers.sh -m <binary> <address_file> <output_dir>

# HTML 리포트 포함
./scripts/compare/compare_decompilers.sh -m -h test.exe addresses.txt results/

# 타임아웃 설정 (기본: 300초)
./scripts/compare/compare_decompilers.sh -m -h -t 600 test.exe addresses.txt results/
```

**주소 파일 형식** (`addresses.txt`):

```
# 형식: <address> [function_name]
0x140001450 add
0x140001470 multiply
0x140001010 main
0x140001490 print_message
```

**매개변수:**

- `<binary>`: 분석할 실행 파일 경로
- `<address>`: 함수 시작 주소 (0x 접두사 포함)
- `<address_file>`: 주소 목록 파일 (배치 모드)
- `[output_json]`: JSON 출력 파일 경로 (v2 단일 모드에서 사용, 미지정 시 `scripts/result/{타임스탬프}_result/` 자동 생성)

**옵션:**

- `-m`: 배치 모드 활성화
- `-h`: HTML 리포트 생성 (배치 모드 전용)
- `-t N`: 타임아웃 설정 (초 단위, 기본: v2=600, v1=300)

**출력 구조:**

```json
{
  "comparison_info": {
    "binary": "test.exe",
    "address": "0x140001450",
    "timestamp": "2026-01-06T10:00:00Z",
    "metrics": {
      "ghidra": {
        "lines": 25,
        "chars": 1234,
        "functions": 5,
        "branches": 3
      },
      "fission": {
        "lines": 30,
        "chars": 1500,
        "functions": 5,
        "branches": 4
      }
    },
    "similarity": 85.5
  },
  "timings": {
    "ghidra_sec": 2.531,
    "fission_asm_sec": 0.312,
    "fission_decomp_sec": 3.102
  },
  "ghidra_assembly": "어셈블리 출력...",
  "ghidra_decompilation": "디컴파일된 C 코드...",
  "fission_assembly": "어셈블리 출력...",
  "fission_decompilation": "디컴파일된 코드..."
}
```

**기능:**

- ✅ ANSI 컬러 코드 자동 제거
- ✅ CLI 도움말 텍스트 필터링
- ✅ 코드 메트릭 자동 분석 (라인 수, 분기문, 함수 호출)
- ✅ 유사도 점수 계산 (0-100%)
- ✅ 타이밍 측정 (Ghidra/Fission)
- ✅ 배치 모드: 여러 함수 일괄 처리
- ✅ HTML 리포트: 테이블 요약 + 링크
- ✅ jq를 통한 미리보기 지원

**결과 확인:**

```bash
# 전체 JSON 보기
OUTPUT_JSON=results/comparison.json
cat "$OUTPUT_JSON" | jq .

# 특정 섹션만 보기
cat "$OUTPUT_JSON" | jq -r '.ghidra_decompilation'
cat "$OUTPUT_JSON" | jq -r '.fission_decompilation'

# 메타데이터 확인
cat "$OUTPUT_JSON" | jq '.comparison_info'

# 메트릭 비교
cat "$OUTPUT_JSON" | jq '.comparison_info.metrics'

# 유사도 점수
cat "$OUTPUT_JSON" | jq '.comparison_info.similarity'

# 타이밍 확인
cat "$OUTPUT_JSON" | jq '.timings'
```

**HTML 리포트 (배치 모드):**

- 📊 타이밍 요약 (avg/median/min/max)
- 🧾 함수별 비교 테이블
- 🔗 Ghidra/Fission 결과 링크

---

### 3. PyGhidra 디컴파일

PyGhidra를 사용하여 단일 함수를 디컴파일하고 어셈블리와 C 코드를 출력합니다.

```bash
python3 scripts/ghidra/pyghidra_decompile.py <binary> <address>

# 예제
python3 scripts/ghidra/pyghidra_decompile.py test.exe 0x401000
python3 scripts/ghidra/pyghidra_decompile.py libfoo.so 0x1234
```

**출력 예시:**

```
=== Ghidra Decompilation (PyGhidra) ===
Binary: test.exe
Address: 0x401000

Function: main
Entry Point: 0x401000

--- Assembly ---
00401000  push    rbp
00401001  mov     rbp,rsp
...

--- Decompiled Code ---
undefined8 main(void) {
    int local_8;
    local_8 = 0;
    ...
    return 0;
}
```

**환경 변수:**

- `GHIDRA_INSTALL_DIR`: Ghidra 설치 경로 (스크립트 내부에 하드코딩 가능)

---

### 4. Ghidra 디컴파일

순수 Ghidra API를 사용하는 독립 실행형 Python 스크립트입니다.

```bash
python3 scripts/ghidra/pyghidra_decompile.py <binary> <address>
```

**PyGhidra와의 차이점:**

- PyGhidra: Python 패키지로 설치, 간편한 API
- ghidra/pyghidra_decompile.py: Ghidra 설치 필요, 저수준 API 접근

---

### 5. FID 데이터베이스 테스트

Function ID (FID) 데이터베이스의 로딩을 테스트하고 사용 가능한 데이터베이스를 확인합니다.

```bash
./scripts/test/test_fid.sh

# 출력 예시
# Testing FID Database Loading
# =============================
#
# Available FID databases:
# -rw-r--r--  1 user  staff   2.1M Jan 1 00:00 vs2019_x64.fidbf
# -rw-r--r--  1 user  staff   1.8M Jan 1 00:00 vs2019_x86.fidbf
# ...
```

**확인 사항:**

- `utils/signatures/fid/*.fidbf` 파일 존재 여부
- Fission native_decomp 기능 빌드 성공

---

### 6. Cppcheck (C++)

```bash
./scripts/lint/cppcheck.sh

# Optional: limit targets
./scripts/lint/cppcheck.sh ghidra_decompiler/src/core
```

---

## 출력 형식

### JSON 출력 (compare_decompilers_v2.sh)

```json
{
  "comparison_info": {
    "binary": "실행 파일 이름",
    "address": "함수 주소",
    "timestamp": "ISO 8601 형식 타임스탬프",
    "metrics": {
      "ghidra": { "lines": 10, "chars": 123, "functions": 3, "branches": 2 },
      "fission": { "lines": 12, "chars": 140, "functions": 3, "branches": 2 }
    },
    "similarity": 92.5
  },
  "timings": {
    "ghidra_sec": 2.531,
    "fission_asm_sec": 0.312,
    "fission_decomp_sec": 3.102
  },
  "ghidra_assembly": "Ghidra 어셈블리 출력",
  "ghidra_decompilation": "Ghidra 디컴파일 결과 (C 코드)",
  "fission_assembly": "Fission 어셈블리 출력",
  "fission_decompilation": "Fission 디컴파일 결과"
}
```

**Legacy (v1):** `compare_decompilers.sh`는 다른 JSON 스키마를 사용합니다.

### 텍스트 출력 (pyghidra_decompile.py)

```
=== Ghidra Decompilation (PyGhidra) ===
Binary: example.exe
Address: 0x401000

Function: function_name
Entry Point: 0x401000

--- Assembly ---
<어셈블리 리스팅>

--- Decompiled Code ---
<C 코드>
```

---

## 문제 해결

### 1. PyGhidra 설치 실패

```bash
# Java 11+ 필요
java -version

# PyGhidra 재설치
pip3 uninstall pyghidra
pip3 install pyghidra --no-cache-dir
```

### 2. "No function found at address" 오류

**원인:** 주소에 함수가 없거나 Ghidra 분석이 불완전

**해결:**

- 올바른 함수 주소 확인 (entry point)
- Binary가 strip되지 않았는지 확인
- Ghidra 분석 완료 대기 (PyGhidra는 자동 분석)

### 3. JSON 출력이 깨지거나 비어있음

**원인:** ANSI 컬러 코드, CLI 도움말 포함

**해결:**

```bash
# 최신 버전의 compare_decompilers_v2.sh 사용
git pull origin main

# 수동 확인
OUTPUT_JSON=results/comparison.json
cat "$OUTPUT_JSON" | jq empty
# 오류 없으면 JSON 형식 올바름
```

### 4. "CMake not found" 오류

```bash
# macOS
brew install cmake

# Linux
sudo apt-get update
sudo apt-get install cmake

# 확인
cmake --version
```

### 5. Ghidra 경로 문제

**pyghidra_decompile.py 수정:**

```python
# Line 14-15
ghidra_path = "/path/to/your/ghidra_11.4.2_PUBLIC"
os.environ['GHIDRA_INSTALL_DIR'] = ghidra_path
```

**또는 환경 변수 설정:**

```bash
export GHIDRA_INSTALL_DIR=/Applications/ghidra_11.4.2_PUBLIC
python3 scripts/ghidra/pyghidra_decompile.py test.exe 0x401000
```

---

## 스크립트 디렉토리 구조

```
scripts/
├── README.md
├── build/
│   ├── build_decompiler.sh
│   ├── build_all_tests.sh
│   └── dev-tools.sh
├── compare/
│   ├── compare_decompilers.sh
│   ├── compare_decompilers_v2.sh
│   ├── compare_decompilers_v2.py
│   ├── extract_functions.sh
│   ├── fix_addresses.py
│   └── example_addresses.txt
├── ghidra/
│   ├── pyghidra_decompile.py
│   ├── pyghidra_decompile_batch.py
│   └── convert_fidb_batch.sh
├── lint/
│   └── cppcheck.sh
└── test/
    ├── test_fid.sh
    ├── run_tests.sh
    ├── run_complex_tests.py
    ├── test_cfg_structurizer.py
    └── test_postprocessor.py
```

---

## 추가 정보

### 성능 벤치마크

```bash
# 100개 함수 비교
for addr in $(seq 0x401000 0x401640 100); do
    ./scripts/compare/compare_decompilers_v2.sh test.exe $(printf "0x%X" $addr) \
        "results/func_$(printf "%X" $addr).json"
done
```

### CI/CD 통합

```yaml
# GitHub Actions example
- name: Compare Decompilers
  run: |
    ./scripts/compare/compare_decompilers_v2.sh examples/binary 0x401000 results/comparison.json
    cat results/comparison.json | jq .
```

### 개발 팁

- **병렬 실행:** `compare_decompilers_v2.sh`는 I/O 집약적이므로 여러 주소를 병렬로 처리 가능
- **캐싱:** Ghidra 분석 결과는 `.gpr` 파일로 캐싱됨
- **디버그:** `--verbose` 플래그 추가 고려 (미래 구현)

---

## 라이선스

Fission 프로젝트 라이선스를 따릅니다. 자세한 내용은 상위 디렉토리의 LICENSE 파일을 참조하세요.

## 기여

버그 리포트 및 개선 제안은 GitHub Issues에 등록해주세요.
