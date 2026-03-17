# Plugin Development Guide

## Overview

Fission's plugin system currently supports **native Rust plugins only** through dynamic libraries.

- Target formats: `.so` (Linux), `.dylib` (macOS), `.dll` (Windows)
- Loading model: dynamic load/unload through `PluginManager`
- Event model: hook dispatch based on `FissionEvent`

The old Python-script / PyO3 plugin runtime has been removed.

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

1. Load the plugin (`load_plugin`)
2. Resolve the `create_plugin` symbol
3. Call `on_load`
4. Receive events and execute callbacks
5. Call `on_unload` during unload

### Runtime Model

- Plugins must satisfy `Send + Sync`
- Per-plugin metadata is managed through `PluginInfo`
- Activation and deactivation are toggled by the manager

---

## Creating a Native Rust Plugin

### 1) `Cargo.toml`

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

### 2) Export the Entry Point

Required symbol:

- `create_plugin`

Recommended symbol:

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

Example output:

- Linux: `target/release/libmy_fission_plugin.so`
- macOS: `target/release/libmy_fission_plugin.dylib`
- Windows: `target/release/my_fission_plugin.dll`

### 4) Load It in Fission

```rust
let mut manager = PluginManager::new();
let id = manager.load_plugin("./target/release/libmy_fission_plugin.so")?;
println!("loaded plugin: {id}");
```

---

## Event System

Representative events:

- `BinaryLoaded`
- `FunctionDecompiled`
- `AnalysisStarted`
- `AnalysisCompleted`
- `DebugEvent`
- `Custom(String)`

Hooks are registered with priorities and dispatched only to active plugins.

---

## Plugin API

Plugins access the host through `PluginContext`, including:

- binary metadata queries
- function and decompilation result queries
- annotation and event integration work

Only the callbacks you need have to be implemented on the `FissionPlugin` trait.

---

## Hook Priorities

Lower values run earlier.

- `Critical`
- `High`
- `Normal`
- `Low`
- `Background`

If multiple hooks subscribe to the same event, they are called in priority order.

---

## Best Practices

- Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared state
- Move heavy work into background tasks
- Do not panic inside callbacks; log and handle errors instead
- Keep ABI-facing public interface changes minimal

---

## Debugging Plugins

### Common Issues

1. Plugin fails to load
   - Check that the `create_plugin` symbol exists
   - Check `crate-type = ["cdylib"]`
   - Check the file extension and path

2. Event callbacks never run
   - Verify the plugin is active
   - Verify event type mapping
   - Verify hook priority registration

3. Runtime crash
   - Recheck thread-safety requirements (`Send + Sync`)
   - Minimize shared-state lock scope
   - Inspect external pointers and FFI boundaries

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

Optionally include a `plugin.toml` metadata file in the distribution.

---

## Related Documentation

- [ARCHITECTURE.md](../architecture/ARCHITECTURE.md)
- [CLI_ONE_SHOT_MODE.md](../cli/CLI_ONE_SHOT_MODE.md)

---

## FAQ

**Q: Can I use Python plugins?**  
A: No. The current plugin runtime supports native Rust plugins only.

**Q: How do I access decompilation output?**  
A: Subscribe to `FunctionDecompiled` or query through `PluginContext`.

**Q: Is the performance overhead large?**  
A: Usually not, as long as callbacks avoid blocking work and push heavy tasks into the background.
