---
description: 디버깅 및 문제 해결 워크플로우
---

# Debug Workflow

Fission 디버깅 및 문제 해결 가이드

---

## 🔍 로그 확인

### Verbose 모드 실행

// turbo

```bash
./target/release/fission_cli <binary> --decomp <address> --verbose 2>&1 | tee debug.log
```

### Rust 로그 레벨 설정

// turbo

```bash
RUST_LOG=debug cargo run -p fission-cli -- <binary> --decomp <address>
```

### 특정 모듈 로그

// turbo

```bash
RUST_LOG=fission_loader=trace cargo run -p fission-cli -- <binary> --info
```

---

## 🐛 일반적인 문제

### 디컴파일 실패

1. **libdecomp.dylib 확인**
// turbo

```bash
ls -la ghidra_decompiler/build/libdecomp.dylib
```

1. **라이브러리 경로 설정**

```bash
export DYLD_LIBRARY_PATH=$PWD/ghidra_decompiler/build:$DYLD_LIBRARY_PATH
```

1. **C++ 재빌드**

```bash
cd ghidra_decompiler/build && cmake .. && make -j
```

### 바이너리 로딩 실패

1. **파일 존재 확인**
// turbo

```bash
file <binary_path>
xxd <binary_path> | head -5
```

1. **매직 바이트 확인**
// turbo

```bash
hexdump -C <binary_path> | head -2
```

### GDT 로딩 실패

1. **GDT 파일 확인**
// turbo

```bash
ls -la utils/signatures/typeinfo/win32/
```

1. **경로 설정 확인**
// turbo

```bash
grep gdt_path crates/fission-ffi/src/decomp.rs
```

---

## 🔬 심화 디버깅

### Rust 백트레이스

// turbo

```bash
RUST_BACKTRACE=1 cargo run -p fission-cli -- <binary> --decomp <address>
```

### 전체 백트레이스

// turbo

```bash
RUST_BACKTRACE=full cargo run -p fission-cli -- <binary> --decomp <address>
```

### Address Sanitizer (C++)

```bash
cd ghidra_decompiler/build
cmake -DCMAKE_CXX_FLAGS="-fsanitize=address" ..
make -j
```

---

## 🧪 단위 테스트 디버깅

### 특정 테스트 실행

// turbo

```bash
cargo test test_name -- --nocapture
```

### 테스트 목록 확인

// turbo

```bash
cargo test -- --list
```

---

## 📊 성능 프로파일링

### 시간 측정

// turbo

```bash
time ./target/release/fission_cli <binary> --decomp <address>
```

### Flamegraph (선택)

```bash
cargo install flamegraph
cargo flamegraph --bin fission_cli -- <binary> --decomp <address>
```

---

## 🔄 코어 덤프 분석

### macOS

```bash
lldb ./target/release/fission
(lldb) run <binary> --decomp <address>
(lldb) bt  # 백트레이스
```

### Linux

```bash
gdb ./target/release/fission
(gdb) run <binary> --decomp <address>
(gdb) bt
```
