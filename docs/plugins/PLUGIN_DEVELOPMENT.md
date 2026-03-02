# Plugin Development Guide

## Overview

Fission 플러그인 시스템은 **Native Rust 플러그인**(동적 라이브러리) 방식만 지원합니다.

- 대상 형식: `.so` (Linux), `.dylib` (macOS), `.dll` (Windows)
- 로딩 방식: `PluginManager`를 통한 동적 로드/언로드
- 이벤트 방식: `FissionEvent` 기반 훅 디스패치

Python 스크립트/PyO3 기반 플러그인 런타임은 제거되었습니다.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Plugin Architecture](#plugin-architecture)
- [Creating a Native Rust Plugin](#creating-a-native-rust-plugin)
- [Event System](#event-system)
- [Plugin API](#plugin-api)
- [Hook Priorities](#hook-priorities)
- [Best Practices](#best-practices)
- [Debugging Plugins](#debugging-plugins)
- [Distribution](#distribution)
- [FAQ](#faq)

---

## Quick Start

```rust
use fission::plugin::{FissionPlugin, PluginContext};
use fission::plugin::api::BinaryInfo;
use fission::core::Result;

pub struct MyPlugin;

impl FissionPlugin for MyPlugin {
    fn id(&self) -> &str { "my_plugin" }
    fn name(&self) -> &str { "My Plugin" }
    fn version(&self) -> &str { "0.1.0" }
    fn description(&self) -> &str { "Example native plugin" }

    fn on_load(&mut self, _ctx: &PluginContext) -> Result<()> {
        println!("plugin loaded");
        Ok(())
    }

    fn on_binary_loaded(&self, _ctx: &PluginContext, info: &BinaryInfo) {
        println!("loaded binary: {}", info.path);
    }
}

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn FissionPlugin {
    Box::into_raw(Box::new(MyPlugin))
}
```

---

## Plugin Architecture

### Lifecycle

1. 플러그인 로드 (`load_plugin`)
2. `create_plugin` 심볼 확인
3. `on_load` 호출
4. 이벤트 수신 및 콜백 실행
5. 언로드 시 `on_unload` 호출

### Runtime Model

- 플러그인은 `Send + Sync`를 만족해야 함
- 플러그인별 메타데이터는 `PluginInfo`로 관리
- 활성/비활성은 매니저에서 토글

---

## Creating a Native Rust Plugin

### 1) Cargo.toml

```toml
[package]
name = "my_fission_plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
fission-core = { path = "../fission-core" }
fission-analysis = { path = "../fission-analysis" }
```

### 2) Export Entry Point

필수 심볼:

- `create_plugin`

권장 심볼:

- `destroy_plugin`

```rust
#[no_mangle]
pub extern "C" fn destroy_plugin(ptr: *mut dyn FissionPlugin) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)); }
    }
}
```

### 3) Build

```bash
cargo build --release
```

출력 예시:

- Linux: `target/release/libmy_fission_plugin.so`
- macOS: `target/release/libmy_fission_plugin.dylib`
- Windows: `target/release/my_fission_plugin.dll`

### 4) Load in Fission

```rust
let mut manager = PluginManager::new();
let id = manager.load_plugin("./target/release/libmy_fission_plugin.so")?;
println!("loaded plugin: {id}");
```

---

## Event System

대표 이벤트:

- `BinaryLoaded`
- `FunctionDecompiled`
- `AnalysisStarted`
- `AnalysisCompleted`
- `DebugEvent`
- `Custom(String)`

이벤트 훅은 우선순위를 갖고 등록되며, 활성화된 플러그인에만 전달됩니다.

---

## Plugin API

플러그인은 `PluginContext`를 통해 API 접근:

- 바이너리 메타 조회
- 함수/디컴파일 결과 조회
- 애노테이션/이벤트 연계 작업

`FissionPlugin` 트레이트에서 필요한 콜백만 구현해도 동작합니다.

---

## Hook Priorities

낮은 값일수록 먼저 실행됩니다.

- `Critical`
- `High`
- `Normal`
- `Low`
- `Background`

복수 훅이 같은 이벤트를 수신하면 우선순위 순으로 호출됩니다.

---

## Best Practices

- 상태 공유는 `Arc<Mutex<T>>` 또는 `Arc<RwLock<T>>` 사용
- 무거운 연산은 백그라운드 작업으로 분리
- 콜백 내부에서 panic 금지, 에러를 로그로 처리
- ABI 호환성 유지를 위해 공개 인터페이스 변경 최소화

---

## Debugging Plugins

### Common Issues

1. 플러그인 로드 실패
   - `create_plugin` 심볼 확인
   - `crate-type = ["cdylib"]` 확인
   - 파일 확장자/경로 확인

2. 이벤트 콜백 미실행
   - 플러그인 활성 상태 확인
   - 이벤트 타입 매핑 확인
   - 등록 훅 우선순위 확인

3. 런타임 크래시
   - 스레드 안전성(`Send + Sync`) 검토
   - 공유 상태 잠금 범위 최소화
   - 외부 포인터/FFI 경계 점검

---

## Distribution

```bash
# Build
cargo build --release

# Package (example)
tar -czf my_plugin.tar.gz \
  target/release/libmy_plugin.so \
  README.md
```

선택적으로 `plugin.toml` 메타데이터 파일을 함께 배포할 수 있습니다.

---

## Related Documentation

- [ARCHITECTURE.md](../architecture/ARCHITECTURE.md)
- [CLI_ONE_SHOT_MODE.md](../cli/CLI_ONE_SHOT_MODE.md)

---

## FAQ

**Q: Python 플러그인을 사용할 수 있나요?**  
A: 아니요. 현재 플러그인 런타임은 Native Rust 플러그인만 지원합니다.

**Q: 디컴파일 결과에 접근하려면?**  
A: `FunctionDecompiled` 이벤트 구독 또는 `PluginContext` API를 사용하세요.

**Q: 성능 영향은 큰가요?**  
A: 콜백에서 블로킹 작업을 피하고 백그라운드 처리하면 영향은 작습니다.
