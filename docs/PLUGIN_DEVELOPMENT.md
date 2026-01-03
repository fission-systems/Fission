# Plugin Development Guide

## Overview

Fission provides a flexible plugin system that allows extending functionality through:
- **Native Rust Plugins** - High-performance, compiled plugins
- **Python Scripts** (optional) - Dynamic scripting via PyO3

Plugins can hook into Fission's event system to react to binary loads, function decompilations, debug events, and more.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Plugin Architecture](#plugin-architecture)
- [Creating a Native Rust Plugin](#creating-a-native-rust-plugin)
- [Creating a Python Plugin](#creating-a-python-plugin)
- [Event System](#event-system)
- [Plugin API](#plugin-api)
- [Hook Priorities](#hook-priorities)
- [Example Plugins](#example-plugins)
- [Best Practices](#best-practices)
- [Debugging Plugins](#debugging-plugins)
- [Distribution](#distribution)

---

## Quick Start

### Native Rust Plugin (5 minutes)

```rust
// my_plugin/src/lib.rs
use fission::plugin::{FissionPlugin, PluginContext};
use fission::core::prelude::*;

pub struct MyPlugin {
    id: String,
}

impl MyPlugin {
    pub fn new() -> Self {
        Self {
            id: "my_plugin".to_string(),
        }
    }
}

impl FissionPlugin for MyPlugin {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "My Custom Plugin"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn description(&self) -> &str {
        "A simple example plugin"
    }

    fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
        println!("Plugin loaded!");
        Ok(())
    }

    fn on_binary_loaded(&self, ctx: &PluginContext, info: &BinaryInfo) {
        println!("Binary loaded: {}", info.path);
    }
}

// Export plugin constructor
#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn FissionPlugin {
    Box::into_raw(Box::new(MyPlugin::new()))
}
```

### Python Plugin (3 minutes)

```python
# my_plugin.py
class MyPlugin:
    def id(self):
        return "my_python_plugin"
    
    def name(self):
        return "My Python Plugin"
    
    def version(self):
        return "0.1.0"
    
    def on_binary_loaded(self, binary_info):
        print(f"Python: Binary loaded - {binary_info.path}")
        print(f"  Format: {binary_info.format}")
        print(f"  Arch: {binary_info.arch}")
```

---

## Plugin Architecture

### Plugin Lifecycle

```
┌──────────────┐
│ Plugin Load  │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  on_load()   │ ◄─── Initialize resources
└──────┬───────┘
       │
       ▼
┌──────────────────────┐
│  Event Registration  │ ◄─── Subscribe to events
└──────┬───────────────┘
       │
       ▼
┌──────────────────┐
│  Event Handling  │ ◄─── Respond to events
└──────┬───────────┘
       │
       ▼
┌──────────────┐
│ on_unload()  │ ◄─── Cleanup
└──────────────┘
```

### Plugin Manager

The `PluginManager` handles:
- Loading/unloading plugins
- Event dispatch
- Priority-based execution
- Thread-safe plugin access

---

## Creating a Native Rust Plugin

### Step 1: Project Setup

```bash
# Create new library project
cargo new --lib my_fission_plugin
cd my_fission_plugin

# Add Fission as dependency
```

**Cargo.toml**:
```toml
[package]
name = "my_fission_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]  # Important: Create dynamic library

[dependencies]
fission = { path = "../fission" }  # Adjust path
anyhow = "1.0"
```

### Step 2: Implement FissionPlugin Trait

```rust
use fission::plugin::{FissionPlugin, PluginContext};
use fission::plugin::api::BinaryInfo;
use fission::core::prelude::*;

pub struct MyPlugin {
    id: String,
    // Plugin state
    analysis_count: std::sync::atomic::AtomicU64,
}

impl MyPlugin {
    pub fn new() -> Self {
        Self {
            id: "example_plugin".to_string(),
            analysis_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl FissionPlugin for MyPlugin {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Example Analysis Plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Demonstrates plugin capabilities"
    }

    fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
        println!("[{}] Plugin loaded", self.name());
        
        // Access Fission API
        if let Some(event_bus) = &ctx.event_bus {
            println!("Event bus available");
        }
        
        Ok(())
    }

    fn on_unload(&mut self, _ctx: &PluginContext) -> Result<()> {
        let count = self.analysis_count.load(std::sync::atomic::Ordering::Relaxed);
        println!("[{}] Analyzed {} functions", self.name(), count);
        Ok(())
    }

    fn on_binary_loaded(&self, ctx: &PluginContext, info: &BinaryInfo) {
        println!("[{}] Binary loaded:", self.name());
        println!("  Path: {}", info.path);
        println!("  Format: {}", info.format);
        println!("  Arch: {}", info.arch);
        println!("  Entry: 0x{:x}", info.entry_point);
    }

    fn on_function_decompiled(&self, ctx: &PluginContext, addr: u64, code: &str) {
        self.analysis_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Analyze decompiled code
        if code.contains("malloc") && code.contains("strcpy") {
            println!("⚠️  Potential buffer overflow at 0x{:x}", addr);
        }
    }
}

// Required: Export plugin constructor
#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn FissionPlugin {
    Box::into_raw(Box::new(MyPlugin::new()))
}

#[no_mangle]
pub extern "C" fn destroy_plugin(plugin: *mut dyn FissionPlugin) {
    unsafe {
        drop(Box::from_raw(plugin));
    }
}
```

### Step 3: Build Plugin

```bash
cargo build --release

# Output: target/release/libmy_fission_plugin.so (Linux)
#         target/release/libmy_fission_plugin.dylib (macOS)
#         target/release/my_fission_plugin.dll (Windows)
```

### Step 4: Load Plugin in Fission

```rust
// In Fission application
let plugin_manager = PluginManager::new();
plugin_manager.load_native_plugin("./target/release/libmy_fission_plugin.so")?;
```

---

## Creating a Python Plugin

### Requirements

Build Fission with Python support:
```bash
cargo build --features python
```

### Basic Python Plugin

```python
# sentiment_analyzer.py
"""
Example: Analyze function names for suspicious keywords
"""

class SuspiciousNameDetector:
    def id(self):
        return "suspicious_name_detector"
    
    def name(self):
        return "Suspicious Function Name Detector"
    
    def version(self):
        return "1.0.0"
    
    def description(self):
        return "Detects suspicious function names (keylog, backdoor, etc.)"
    
    def on_load(self, ctx):
        print(f"[{self.name()}] Loaded")
        self.suspicious_keywords = [
            "keylog", "backdoor", "rootkit", "inject",
            "elevate", "bypass", "crack", "steal"
        ]
    
    def on_binary_loaded(self, binary_info):
        print(f"[{self.name()}] Analyzing: {binary_info.path}")
        
        # Access functions via API
        functions = binary_info.functions
        suspicious = []
        
        for func in functions:
            name = func.name.lower()
            for keyword in self.suspicious_keywords:
                if keyword in name:
                    suspicious.append((func.address, func.name))
                    break
        
        if suspicious:
            print(f"⚠️  Found {len(suspicious)} suspicious functions:")
            for addr, name in suspicious:
                print(f"    0x{addr:x}: {name}")
    
    def on_function_decompiled(self, addr, code):
        # Check for suspicious patterns in decompiled code
        if "WriteProcessMemory" in code:
            print(f"⚠️  Potential code injection at 0x{addr:x}")
```

### Advanced Python Plugin with API Usage

```python
# code_coverage.py
"""
Track decompiled function coverage
"""

class CodeCoverageTracker:
    def __init__(self):
        self.decompiled_functions = set()
        self.total_functions = 0
    
    def id(self):
        return "code_coverage_tracker"
    
    def name(self):
        return "Code Coverage Tracker"
    
    def version(self):
        return "1.0.0"
    
    def on_binary_loaded(self, binary_info):
        self.total_functions = len(binary_info.functions)
        print(f"📊 Total functions: {self.total_functions}")
    
    def on_function_decompiled(self, addr, code):
        self.decompiled_functions.add(addr)
        coverage = (len(self.decompiled_functions) / self.total_functions) * 100
        print(f"📈 Coverage: {coverage:.1f}% ({len(self.decompiled_functions)}/{self.total_functions})")
    
    def on_unload(self, ctx):
        print(f"[{self.name()}] Final coverage: {len(self.decompiled_functions)}/{self.total_functions}")
```

### Loading Python Plugins

```rust
// In Fission application
#[cfg(feature = "python")]
{
    let plugin_manager = PluginManager::new();
    plugin_manager.load_python_plugin("./plugins/sentiment_analyzer.py")?;
    plugin_manager.load_python_plugin("./plugins/code_coverage.py")?;
}
```

---

## Event System

### Available Events

```rust
pub enum FissionEventType {
    /// Binary file loaded
    BinaryLoaded,
    
    /// Function decompiled
    FunctionDecompiled,
    
    /// Analysis started
    AnalysisStarted,
    
    /// Analysis completed
    AnalysisCompleted,
    
    /// Debug event (breakpoint, step, etc.)
    DebugEvent,
    
    /// Custom plugin event
    Custom(String),
}
```

### Event Structure

```rust
pub struct FissionEvent {
    pub event_type: FissionEventType,
    pub timestamp: std::time::SystemTime,
    pub data: EventData,
}

pub enum EventData {
    BinaryInfo(BinaryInfo),
    FunctionDecompiled { addr: u64, code: String },
    DebugInfo(DebugEvent),
    Custom(serde_json::Value),
}
```

### Subscribing to Events

```rust
impl FissionPlugin for MyPlugin {
    fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
        if let Some(event_bus) = &ctx.event_bus {
            // Subscribe to events
            let event_rx = event_bus.subscribe()?;
            
            // Handle events in background thread
            std::thread::spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    match event.event_type {
                        FissionEventType::BinaryLoaded => {
                            // Handle binary load
                        }
                        FissionEventType::FunctionDecompiled => {
                            // Handle decompilation
                        }
                        _ => {}
                    }
                }
            });
        }
        Ok(())
    }
}
```

---

## Plugin API

### BinaryInfo Structure

```rust
pub struct BinaryInfo {
    pub path: String,
    pub format: String,       // "PE", "ELF", "Mach-O"
    pub arch: String,          // "x86", "x86_64", "ARM"
    pub entry_point: u64,
    pub image_base: u64,
    pub sections: Vec<Section>,
    pub functions: Vec<Function>,
}
```

### API Methods

```rust
pub trait PluginAPI: Send + Sync {
    /// Get current binary info
    fn get_binary_info(&self) -> Option<BinaryInfo>;
    
    /// Request function decompilation
    fn decompile_function(&self, addr: u64) -> Result<String>;
    
    /// Get function list
    fn get_functions(&self) -> Vec<Function>;
    
    /// Add custom annotation
    fn add_annotation(&self, addr: u64, text: String);
}
```

### Using the API

```rust
impl FissionPlugin for MyPlugin {
    fn on_binary_loaded(&self, ctx: &PluginContext, info: &BinaryInfo) {
        // Get functions
        let functions = ctx.api.get_functions();
        
        // Decompile interesting functions
        for func in functions {
            if func.name.contains("crypto") {
                if let Ok(code) = ctx.api.decompile_function(func.address) {
                    println!("Crypto function:\n{}", code);
                }
            }
        }
    }
}
```

---

## Hook Priorities

Control execution order when multiple plugins handle the same event:

```rust
pub enum HookPriority {
    High = 0,    // Execute first
    Normal = 50, // Default
    Low = 100,   // Execute last
}
```

Plugins with `High` priority run before `Normal`, which run before `Low`.

**Use cases**:
- **High**: Pre-processing, validation, early filters
- **Normal**: Standard analysis
- **Low**: Post-processing, reporting, cleanup

---

## Example Plugins

### 1. String Extractor

```rust
pub struct StringExtractor {
    id: String,
    strings: Arc<Mutex<Vec<String>>>,
}

impl FissionPlugin for StringExtractor {
    fn id(&self) -> &str { "string_extractor" }
    fn name(&self) -> &str { "String Extractor" }

    fn on_function_decompiled(&self, _ctx: &PluginContext, _addr: u64, code: &str) {
        let re = regex::Regex::new(r#""([^"]+)""#).unwrap();
        let mut strings = self.strings.lock().unwrap();
        
        for cap in re.captures_iter(code) {
            strings.push(cap[1].to_string());
        }
    }
}
```

### 2. API Call Monitor

```python
class ApiCallMonitor:
    def __init__(self):
        self.api_calls = {}
    
    def on_function_decompiled(self, addr, code):
        # Track Windows API calls
        apis = ["CreateFile", "WriteFile", "VirtualAlloc", "CreateProcess"]
        
        for api in apis:
            if api in code:
                self.api_calls[api] = self.api_calls.get(api, 0) + 1
        
    def on_unload(self, ctx):
        print("API Call Summary:")
        for api, count in sorted(self.api_calls.items()):
            print(f"  {api}: {count} calls")
```

---

## Best Practices

### 1. Thread Safety
All plugins must be `Send + Sync`:
```rust
use std::sync::{Arc, Mutex};

pub struct ThreadSafePlugin {
    state: Arc<Mutex<PluginState>>,
}
```

### 2. Error Handling
Never panic - return `Result`:
```rust
fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
    let file = std::fs::File::open("config.json")
        .map_err(|e| anyhow::anyhow!("Config load failed: {}", e))?;
    Ok(())
}
```

### 3. Resource Cleanup
Always clean up in `on_unload`:
```rust
fn on_unload(&mut self, _ctx: &PluginContext) -> Result<()> {
    self.save_state()?;
    self.close_connections()?;
    Ok(())
}
```

### 4. Performance
- Avoid blocking operations in event handlers
- Use background threads for heavy work
- Cache expensive computations

```rust
fn on_function_decompiled(&self, ctx: &PluginContext, addr: u64, code: &str) {
    let code = code.to_string();
    std::thread::spawn(move || {
        // Heavy analysis in background
        expensive_analysis(&code);
    });
}
```

---

## Debugging Plugins

### Enable Debug Logging

```bash
RUST_LOG=debug cargo run
```

### Print Debugging

```rust
impl FissionPlugin for MyPlugin {
    fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
        eprintln!("[DEBUG] Plugin loading...");
        Ok(())
    }
}
```

### Use Rust Debugger

```bash
# In VS Code: launch.json
{
    "type": "lldb",
    "request": "launch",
    "name": "Debug Fission with Plugins",
    "program": "${workspaceFolder}/target/debug/fission",
    "args": ["--cli", "test.exe"]
}
```

### Common Issues

1. **Plugin not loading**
   - Check `create_plugin` export
   - Verify `crate-type = ["cdylib"]`
   - Check library path

2. **Crashes on event**
   - Check thread safety
   - Validate API calls
   - Handle errors properly

3. **Python plugin errors**
   - Verify Python feature enabled
   - Check Python syntax
   - Ensure class name matches filename

---

## Distribution

### Native Plugin

```bash
# Build release
cargo build --release

# Package
tar -czf my_plugin.tar.gz \
    target/release/libmy_plugin.so \
    README.md \
    LICENSE
```

### Python Plugin

```bash
# Create package
mkdir my_plugin
cp my_plugin.py my_plugin/
cp README.md LICENSE my_plugin/

zip -r my_plugin.zip my_plugin/
```

### Plugin Manifest (Optional)

```toml
# plugin.toml
[plugin]
id = "my_plugin"
name = "My Plugin"
version = "1.0.0"
author = "Your Name"
description = "Plugin description"

[requirements]
fission_version = ">=0.1.0"
features = ["python"]  # Optional features needed

[files]
native = "libmy_plugin.so"
python = "my_plugin.py"
```

---

## Related Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [CLI_ONE_SHOT_MODE.md](CLI_ONE_SHOT_MODE.md) - CLI usage

---

## Community Plugins

Share your plugins:
- GitHub: https://github.com/sjkim1127/Fission/discussions
- Submit PR to add to official plugin list

**Template Repository**: Coming soon

---

## FAQ

**Q: Can I mix Rust and Python plugins?**  
A: Yes, they work together seamlessly.

**Q: How do I access decompiler output?**  
A: Subscribe to `FunctionDecompiled` events or use `ctx.api.decompile_function(addr)`.

**Q: Can plugins modify decompiled code?**  
A: Not directly, but you can add annotations via `add_annotation()`.

**Q: Thread safety requirements?**  
A: All plugins must implement `Send + Sync`. Use `Arc<Mutex<T>>` for shared state.

**Q: Performance impact?**  
A: Minimal if implemented correctly. Use background threads for heavy work.
