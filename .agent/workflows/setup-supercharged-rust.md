---
description: Fission 프로젝트를 위한 고성능 AI 협업 환경(Bacon, Insta, Criterion, Console) 세팅 가이드
---

# 고성능 Rust 개발 환경 구축 워크플로우

이 워크플로우는 Fission 프로젝트에서 AI와의 협업 효율을 극대화하고 코드 품질을 보장하기 위한 도구들을 설정하는 과정을 다룹니다.

## 1. Project Context 강화 (.cursorrules)

AI가 프로젝트의 아키텍처와 컨벤션을 준수하도록 규칙을 설정합니다.

1. 프로젝트 루트에 `.cursorrules` 파일을 생성합니다.
2. 다음 핵심 내용을 포함시킵니다:
   - **아키텍처**: `fission-core` 중심의 의존성 관리.
   - **에러 처리**: `unwrap()` 지양, `FissionError` 사용.
   - **성능**: `rkyv` Zero-Copy, `rayon` 병렬 처리 우선.
   - **테스트**: `insta` 스냅샷 테스트 권장.

## 2. 실시간 검증 및 회귀 테스트 (Bacon & Insta)

### Bacon 설치 및 실행

실시간 컴파일 피드백을 위해 `bacon`을 사용합니다.

```bash
cargo install bacon
bacon
```

### Insta 스냅샷 테스트 설정

복잡한 데이터 구조의 변경을 감지하기 위해 `insta`를 설정합니다.

1. `cargo-insta` CLI 도구를 설치합니다.

   ```bash
   cargo install cargo-insta
   ```

2. 테스트 코드에 `insta::assert_yaml_snapshot!`을 추가합니다.

   ```rust
   #[test]
   fn test_snapshot() {
       let data = calculate_complex_data();
       insta::assert_yaml_snapshot!(data);
   }
   ```

3. 테스트 실행 후 생성된 스냅샷을 검토하고 승인합니다.

   ```bash
   cargo insta review
   ```

## 3. 성능 벤치마킹 (Criterion)

로직 최적화의 기준점을 마련하기 위해 `criterion`을 설정합니다.

1. 타겟 크레이트의 `Cargo.toml`에 `criterion` 의존성을 추가하고 `[[bench]]` 섹션을 설정합니다.

   ```toml
   [dev-dependencies]
   criterion = "0.5"

   [[bench]]
   name = "benchmark"
   harness = false
   ```

2. `benches/benchmark.rs` 파일을 생성하고 벤치마크 코드를 작성합니다.
3. 벤치마크를 실행합니다.

   ```bash
   cargo bench -p <crate-name>
   ```

## 4. 비동기 디버깅 (Tokio Console)

비동기 작업의 데드락과 병목을 시각화하기 위해 `console-subscriber`를 설정합니다.

1. `Cargo.toml`에 `console-subscriber` 의존성과 `tokio-console` 기능을 추가합니다.

   ```toml
   [features]
   tokio-console = ["dep:console-subscriber"]
   ```

2. `main.rs`의 진입점에 초기화 코드를 추가합니다.

   ```rust
   #[cfg(feature = "tokio-console")]
   console_subscriber::init();
   ```

3. `tokio_unstable` 플래그와 함께 실행하여 모니터링합니다.

   ```bash
   RUSTFLAGS="--cfg tokio_unstable" cargo run --features tokio-console
   ```
