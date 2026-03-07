---
description: FFI 바인딩 추가 워크플로우 (Rust ↔ C++)
---

# Add FFI Binding Workflow

Rust와 C++ 간 새 FFI 함수 추가 가이드

---

## 📋 기존 FFI 구조 확인

### 파일 위치

| 파일 | 역할 |
|------|------|
| `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h` | C 함수 선언 |
| `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp` | C++ 구현 |
| `crates/fission-ffi/src/decomp.rs` | Rust 바인딩 |

### 현재 FFI 함수 확인

// turbo

```bash
grep "DECOMP_API" ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h | head -20
```

---

## 🏗️ 1단계: C 헤더에 선언 추가

`libdecomp_ffi.h`:

```c
/**
 * 함수 설명
 * @param ctx 디컴파일러 컨텍스트
 * @param arg1 인자 설명
 * @return 반환값 설명
 */
DECOMP_API DecompError decomp_new_function(
    DecompContext* ctx,
    const char* arg1,
    uint64_t arg2
);
```

---

## 🔧 2단계: C++ 구현 추가

`libdecomp_ffi.cpp`:

```cpp
extern "C" DECOMP_API DecompError decomp_new_function(
    DecompContext* ctx,
    const char* arg1,
    uint64_t arg2
) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    try {
        // 구현
        return DECOMP_OK;
    } catch (...) {
        return DECOMP_ERR_DECOMPILE;
    }
}
```

---

## 🦀 3단계: Rust 바인딩 추가

`decomp.rs` - extern 블록:

```rust
extern "C" {
    pub fn decomp_new_function(
        ctx: *mut DecompContext,
        arg1: *const c_char,
        arg2: u64,
    ) -> DecompError;
}
```

---

## 📦 4단계: Rust 래퍼 추가

`decomp.rs` - DecompilerNative impl:

```rust
impl DecompilerNative {
    pub fn new_function(&self, arg1: &str, arg2: u64) -> Result<(), DecompError> {
        let c_arg1 = CString::new(arg1).map_err(|_| DecompError::ErrInit)?;
        
        let result = unsafe {
            decomp_new_function(self.ctx, c_arg1.as_ptr(), arg2)
        };
        
        if result == DecompError::Ok {
            Ok(())
        } else {
            Err(result)
        }
    }
}
```

---

## 🔨 5단계: 빌드

### C++ 빌드

```bash
cd ghidra_decompiler/build && make -j
```

### Rust 빌드

// turbo

```bash
cargo build -p fission-ffi
```

---

## ✅ 6단계: 테스트

### FFI 테스트 실행

// turbo

```bash
cargo test -p fission-ffi
```

### 통합 테스트

// turbo

```bash
cargo run -p fission-cli -- tests/binaries/test_swift_bin --info
```

---

## 🐛 문제 해결

### 링크 에러

```bash
# 라이브러리 경로 확인
export DYLD_LIBRARY_PATH=$PWD/ghidra_decompiler/build
```

### 심볼 누락

```bash
# nm으로 심볼 확인
nm ghidra_decompiler/build/libdecomp.dylib | grep decomp_new_function
```
