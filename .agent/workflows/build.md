---
description: Rust 및 C++ 코드 빌드 워크플로우
---

# Build Workflow

Fission 프로젝트 빌드 단계별 가이드

---

## 🔧 전체 빌드

### 1. C++ 디컴파일러 빌드

```bash
cd ghidra_decompiler/build
cmake ..
make -j$(nproc)
```

### 2. Rust 릴리즈 빌드

// turbo

```bash
cargo build --release
```

### 3. 빌드 확인

// turbo

```bash
ls -la target/release/fission
ls -la ghidra_decompiler/build/libdecomp.dylib
```

---

## 🚀 개발 빌드 (빠른 반복)

### Debug 빌드

// turbo

```bash
cargo build
```

### 특정 크레이트만 빌드

// turbo

```bash
cargo build -p fission-loader
cargo build -p fission-analysis
cargo build -p fission-ffi
cargo build -p fission-ui
```

---

## 🧹 클린 빌드

### Rust 클린

// turbo

```bash
cargo clean
```

### C++ 클린

```bash
cd ghidra_decompiler/build
make clean
```

### 전체 클린 후 재빌드

```bash
cargo clean
cd ghidra_decompiler/build && make clean && cmake .. && make -j
cd ../..
cargo build --release
```

---

## ✅ 빌드 검증

### 테스트 실행

// turbo

```bash
cargo test
```

### 특정 크레이트 테스트

// turbo

```bash
cargo test -p fission-pcode
cargo test -p fission-loader
```

### Clippy 린트

// turbo

```bash
cargo clippy --all-targets
```

---

## 🐛 빌드 문제 해결

### libdecomp.dylib not found

```bash
export DYLD_LIBRARY_PATH=$PWD/ghidra_decompiler/build:$DYLD_LIBRARY_PATH
```

### CMake 에러

```bash
cd ghidra_decompiler/build
rm -rf CMakeCache.txt CMakeFiles
cmake ..
```

### Rust 버전 확인

// turbo

```bash
rustc --version
cargo --version
```
