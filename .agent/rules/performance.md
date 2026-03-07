---
trigger: manual
---

# 성능 최적화 가이드

이 규칙은 `/performance` 명령으로 수동 호출됩니다.

## 벤치마크 실행

```bash
# 단일 함수 시간 측정
time ./target/release/fission_cli <binary> --decomp <address>

# 복수 함수 벤치마크
python3 scripts/run_complex_tests.py
```

## 프로파일링

### Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bin fission_cli -- <binary> --decomp <address>
```

### perf (Linux)

```bash
perf record ./target/release/fission_cli <binary> --decomp <address>
perf report
```

## 일반적인 병목

### 1. 문자열 할당

- `String::from()` 대신 `&str` 사용 (가능한 경우)
- `format!()` 최소화
- `String::with_capacity()` 사전 할당

### 2. 해시맵

- `HashMap` 대신 `FxHashMap` (rustc-hash)
- 작은 컬렉션은 `Vec` 선형 검색이 더 빠를 수 있음

### 3. 정규식

- 컴파일된 정규식 재사용 (`lazy_static!` 또는 `once_cell`)
- 단순 패턴은 문자열 메서드 사용

### 4. I/O

- 버퍼링된 I/O 사용 (`BufReader`, `BufWriter`)
- 대용량 파일은 mmap 고려

### 5. FFI

- FFI 호출 최소화
- 배치 처리 가능한 경우 한 번에 전달

## 컴파일러 최적화

### 릴리즈 빌드 설정 (Cargo.toml)

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
```

### 링크 타임 최적화

```toml
[profile.release]
lto = true
```
