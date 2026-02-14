---
description: Fission CLI 단일 실행 및 비교 벤치마크 워크플로우
---

# CLI Oneshot Workflow

Fission CLI를 사용한 단일 디컴파일 및 Ghidra 비교 벤치마크 실행 방법

---

## 🚀 기본 CLI 사용법

### 1. 바이너리 정보 확인

// turbo

```bash
./target/release/fission --cli <binary_path> info
```

### 2. 함수 목록 조회

// turbo

```bash
./target/release/fission --cli <binary_path> functions
```

### 3. 특정 주소 디컴파일

// turbo

```bash
./target/release/fission --cli <binary_path> <address>
```

### 4. Verbose 모드

// turbo

```bash
./target/release/fission --cli <binary_path> <address> -v
```

---

## 📊 Ghidra 비교 벤치마크

### 스크립트 위치

```
scripts/compare/
├── compare_decompilers_v2.py    # 메인 비교 스크립트
├── compare_decompilers_v2.sh    # 실행 래퍼
└── example_addresses.txt        # 예제 주소 목록
```

### 비교 스크립트 실행

1. **단일 함수 비교**
// turbo

```bash
cd scripts/compare
python3 compare_decompilers_v2.py --binary <path> --address <addr>
```

1. **복수 함수 비교 (주소 파일)**
// turbo

```bash
cd scripts/compare
python3 compare_decompilers_v2.py --binary <path> --addresses example_addresses.txt
```

1. **래퍼 스크립트 사용**
// turbo

```bash
cd scripts/compare
./compare_decompilers_v2.sh
```

---

## 🧪 테스트 바이너리

### 위치

```
tests/binaries/
├── test_swift.swift    + test_swift_bin      # Swift
├── test_go.go          + test_go_bin         # Go
├── test_objc.m         + test_objc_bin       # Objective-C
└── test_rust.rs        + test_rust_bin       # Rust
```

### 테스트 실행 예시

#### Swift 디컴파일

// turbo

```bash
./target/release/fission --cli tests/binaries/test_swift_bin 0x1000010a4
```

#### Go 바이너리 정보

// turbo

```bash
./target/release/fission --cli tests/binaries/test_go_bin info
```

#### ObjC 디컴파일

// turbo

```bash
./target/release/fission --cli tests/binaries/test_objc_bin functions
```

---

## 🔧 빌드 및 준비

### 릴리즈 빌드

// turbo

```bash
cargo build --release
```

### C++ 디컴파일러 빌드

```bash
cd ghidra_decompiler/build
cmake ..
make -j
```

### GUI 실행

// turbo

```bash
./target/release/fission --gui
```

---

## 📋 문제 해결

### "Binary too small" 에러

- 바이너리 경로 확인
- 파일이 실제 실행 파일인지 확인

### "Invalid address format" 에러

- 주소는 `0x` 접두사 포함 16진수
- 예: `0x100001234`

### 디컴파일 결과가 비어있음

- C++ 라이브러리 빌드 확인: `ls ghidra_decompiler/build/libdecomp.dylib`
- GDT 경로 확인: `utils/signatures/typeinfo/`

---

## 📈 성능 측정

### 단일 함수 시간 측정

// turbo

```bash
time ./target/release/fission --cli <binary> <address>
```

### 복수 함수 벤치마크

// turbo

```bash
python3 scripts/run_complex_tests.py
```
