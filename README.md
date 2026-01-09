# Fission

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Security](https://github.com/sjkim1127/Fission/actions/workflows/security.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)]()

> **"Split the Binary, Fuse the Power."**

A next-generation hybrid dynamic analysis platform that unifies the best features of x64dbg, Frida, Radare2, and Ghidra into a single high-performance Rust-powered binary.

<!--
TODO: Add screenshot when available
![Fission Screenshot](docs/screenshot.png)
-->

---

## Table of Contents

- [Overview](#overview)
- [Target Users](#target-users)
- [Features](#features)
- [Screenshots](#screenshots)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Usage](#usage)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Plugin Development](#plugin-development)
- [Tech Stack](#tech-stack)
- [Performance](#performance)
- [Comparison with Other Tools](#comparison-with-other-tools)
- [Project Structure](#project-structure)
- [Development Status](#development-status)
- [Implementation Notes](#implementation-notes)
- [Roadmap](#roadmap)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [FAQ](#faq)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)
- [Acknowledgments](#acknowledgments)
- [Documentation](#documentation)

---

## Overview

**Fission** is a comprehensive reverse engineering and binary analysis platform built entirely in Rust. It combines static analysis capabilities (disassembly, decompilation) with dynamic analysis features (debugging, time-travel debugging) in a single, unified tool.

### Why Fission?

- **Unified Experience**: No need to switch between multiple tools - disassembly, decompilation, and debugging in one place
- **High Performance**: Rust-powered core with multi-process decompiler pool and intelligent caching
- **Cross-Platform**: Native support for Windows (PE), Linux (ELF), and macOS (Mach-O) binaries
- **Extensible**: Plugin system supporting both Rust and Python for custom analysis workflows
- **Modern UI**: VS Code-inspired interface with Catppuccin theming

---

## Target Users

- **Malware Analysts** - Analyze suspicious binaries with static and dynamic analysis
- **Vulnerability Researchers** - Find and understand security vulnerabilities in binaries
- **Reverse Engineers** - Understand proprietary software, recover algorithms, and analyze protocols
- **CTF Players** - Solve reverse engineering challenges efficiently
- **Security Auditors** - Audit compiled applications for security issues

---

## Features

### Static Analysis

| Feature | Description |
|---------|-------------|
| **Ghidra-Powered Decompiler** | High-performance C code decompilation via direct FFI integration with optimized Pcode IR |
| **iced-x86 Disassembler** | Pure Rust x86/x64 disassembly with syntax highlighting |
| **.NET Binary Support** | CLR metadata parsing, IL disassembly, native stub analysis |
| **Cross-Platform Binaries** | Windows (PE), Linux (ELF), and macOS (Mach-O) support |
| **Cross-Reference Analysis** | Automatic code and data cross-reference detection |
| **String Extraction** | ASCII and Unicode string detection with context |

### Dynamic Analysis

| Feature | Description |
|---------|-------------|
| **Process Debugging** | Attach/detach to running processes with full control |
| **Breakpoints** | Software breakpoints with hit counting and conditions |
| **Register/Memory Access** | View and modify CPU registers and process memory |
| **Time Travel Debugging** | Execution timeline with snapshot navigation and replay |
| **Live Memory Patching** | Modify running process memory in real-time |

### Smart Decompilation

| Feature | Description |
|---------|-------------|
| **Context-Aware Constant Substitution** | Replaces magic numbers with symbolic names based on API parameter context |
| **16 Enum Groups** | PAGE_PROTECT, MEM_ALLOC, GENERIC_ACCESS, HKEY_ROOT, AF_FAMILY, and more |
| **100+ API Mappings** | Coverage for 9 DLLs (kernel32, user32, ntdll, advapi32, ws2_32, winhttp, wininet, shell32, bcrypt) |
| **Dynamic Flag Resolution** | Automatically detects OR combinations (e.g., `0x3000` → `MEM_COMMIT \| MEM_RESERVE`) |
| **GDT Type Loading** | 5,700+ structures and 6,500+ typedefs from Ghidra data |
| **Advanced Pcode Optimization** | Def-Use chains, CSE, Dead Code Elimination, and pointer arithmetic simplification |

### Advanced Type Analysis

| Feature | Description |
|---------|-------------|
| **Auto-Inferred Structures** | Automatically detects structure layouts and generates C `typedef` definitions |
| **Reverse Type Propagation** | Propagates inferred types from callees back to callers for better variable typing |
| **Smart String Recovery** | Converts hex constants (`0x6d65...`) into readable string literals (`"TestItem"`) |
| **VTable Analysis** | Recovers C++ virtual tables and resolves indirect calls (`call [rax+0x10]` → `Class::method`) |
| **Precise Field Typing** | Distinguishes `float`/`double` fields via FPU instruction analysis |

### Performance Optimization

| Feature | Description |
|---------|-------------|
| **Direct FFI Integration** | Zero-copy decompilation via native C++ bindings (no IPC overhead) |
| **Pcode IR Optimizer** | 32+ optimization rules with def-use tracking and NZMask analysis |
| **Architecture Caching** | Reuses Ghidra objects across requests |
| **LRU Result Cache** | Configurable cache with automatic eviction (default: 100 entries) |
| **Advanced Optimizations** | Constant folding, dead code elimination, shift-bitops, AND-mask optimization |
| **Background Prefetching** | Pre-decompiles adjacent functions for faster navigation |

### Extensibility

| Feature | Description |
|---------|-------------|
| **Plugin System** | Native Rust and Python plugin support |
| **Event Bus** | Subscribe to binary load, decompile, and debug events |
| **Hook Priority** | Control plugin execution order |
| **Python Scripting API** | Full access to binary info, functions, sections via PyO3 |

---

## Screenshots

> Screenshots coming soon. The GUI features a VS Code-inspired layout with:
> - Left sidebar with function explorer and search
> - Center editor with tabbed Assembly and Decompiled C views
> - Bottom panel with Console, Debug, Hex View, and Timeline tabs
> - Catppuccin theme for comfortable viewing

---

## Installation

### Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust | 1.85+ | Install via [rustup](https://rustup.rs/) |
| CMake | 3.16+ | For building Ghidra decompiler |
| C++ Compiler | See below | Platform-specific |
| vcpkg | Latest | Windows only, for ZLIB |

### Windows

```powershell
# 1. Install Visual Studio 2022 with C++ workload
#    - Download from https://visualstudio.microsoft.com/
#    - Select "Desktop development with C++" workload

# 2. Install Rust
winget install Rustlang.Rustup
# Or download from https://rustup.rs/

# 3. Install CMake
winget install Kitware.CMake
# Or download from https://cmake.org/download/

# 4. Install vcpkg and ZLIB
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install zlib:x64-windows

# 5. Set environment variable
$env:VCPKG_ROOT = "C:\vcpkg"
# Add to system environment variables for persistence

# 6. Clone and build Fission
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# 7. Build Ghidra decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_TOOLCHAIN_FILE=C:\vcpkg\scripts\buildsystems\vcpkg.cmake
cmake --build build --config Release
cd ..

# 8. Build and run Fission
cargo build --release
cargo run --release
```

### Linux (Ubuntu/Debian)

```bash
# 1. Install dependencies
sudo apt update
sudo apt install -y build-essential cmake pkg-config zlib1g-dev libssl-dev \
    libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libfontconfig1-dev

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Clone and build Fission
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# 4. Build Ghidra decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
cd ..

# 5. Build and run Fission
cargo build --release
cargo run --release
```

### Linux (Fedora/RHEL)

```bash
# 1. Install dependencies
sudo dnf install -y gcc gcc-c++ cmake pkg-config zlib-devel openssl-devel \
    gtk3-devel libxcb-devel libxkbcommon-devel fontconfig-devel

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Clone and build (same as Ubuntu)
git clone https://github.com/sjkim1127/Fission.git
cd Fission
cd ghidra_decompiler && cmake -B build && cmake --build build && cd ..
cargo build --release
```

### macOS

```bash
# 1. Install Xcode Command Line Tools
xcode-select --install

# 2. Install Homebrew (if not installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 3. Install dependencies
brew install cmake pkg-config zlib

# 4. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 5. Clone and build Fission
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# 6. Build Ghidra decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
cd ..

# 7. Build and run Fission
cargo build --release
cargo run --release
```

### Feature Flags

Fission supports optional features that can be enabled/disabled at compile time:

```bash
# Default build (GUI + CLI)
cargo build --release

# CLI only (smaller binary, no GUI dependencies)
cargo build --release --no-default-features --features cli

# GUI only
cargo build --release --no-default-features --features gui

# With Python scripting support
cargo build --release --features python

# With terminal UI
cargo build --release --features "tui,native_decomp"

# All features
cargo build --release --features "gui cli python tui native_decomp"
```

---

## Quick Start

### GUI Mode (Default)

```bash
# Launch the GUI
cargo run --release

# Or run the compiled binary directly
./target/release/fission
```

1. **Open a Binary**: File → Open Binary (or drag and drop)
2. **Browse Functions**: Use the Explorer panel on the left
3. **View Disassembly**: Double-click a function to see Assembly
4. **View Decompiled Code**: Switch to the "Decompiled" tab
5. **Debug**: Use the Debug panel to attach to a process

### CLI Mode (Headless)

```bash
# Launch in headless/CLI mode
cargo run --release -- --cli <binary_path>

# Or run the compiled binary directly
./target/release/fission --cli <binary_path>

# Quick analysis without entering REPL
./target/release/fission --cli binary.exe --info
./target/release/fission --cli binary.exe --sections
./target/release/fission --cli binary.exe --strings
./target/release/fission --cli binary.exe --xrefs 0x140001000

# With increased verbosity
cargo run --release -- --cli binary.exe -vvv
```

### TUI Mode (Terminal UI)

```bash
# Build and launch the terminal UI
cargo run --release --bin fission_tui --features "tui,native_decomp" -- <binary_path>

# Example
cargo run --release --bin fission_tui --features "tui,native_decomp" -- test/comparison_test_x64.exe
```

---

## Usage

### GUI Mode Workflow

#### 1. Loading a Binary

- **File → Open Binary**: Opens a file dialog
- **Drag and Drop**: Drop a file onto the main window
- **Recent Files**: Access recently opened binaries from File menu

#### 2. Function Explorer

The left sidebar shows all discovered functions:
- **Imports**: Functions imported from external libraries
- **Exports**: Functions exported by the binary
- **Internal**: Functions discovered through analysis
- Use the search box to filter functions by name or address

#### 3. Disassembly View

- Double-click a function to open it in the editor
- Assembly tab shows x86/x64 disassembly with syntax highlighting
- Click on addresses to navigate to referenced locations
- Right-click for context menu options

#### 4. Decompilation View

- Switch to "Decompiled" tab to see C-like code
- Magic numbers are automatically replaced with symbolic constants
- Hover over variables for type information
- Right-click to rename variables or add comments

#### 5. Debugging

1. **Attach to Process**: Debug → Attach to Process
2. **Set Breakpoints**: Click in the gutter or use F9
3. **Control Execution**: F5 (Continue), F10 (Step Over), F11 (Step Into)
4. **Inspect State**: View registers, memory, and stack in Debug panel

#### 6. Time Travel Debugging

1. Enable TTD recording from Debug menu
2. Execute the program
3. Use Timeline panel to navigate execution history
4. Click on any point to restore program state

### CLI Mode Commands

| Command | Aliases | Syntax | Description |
|---------|---------|--------|-------------|
| `load` | `open`, `o` | `load <path>` | Load a binary file |
| `info` | `i` | `info` | Display binary information (format, architecture, entry point) |
| `funcs` | `functions`, `f` | `funcs` | List all discovered functions |
| `sections` | `sec` | `sections` | Show section table with permissions |
| `strings` | `str` | `strings` | Extract printable strings (min 4 chars) |
| `analyze` | `anal`, `a` | `analyze` | Run function discovery analysis |
| `disasm` | `dis`, `d` | `disasm <addr> [count]` | Disassemble instructions at address |
| `decompile` | `dec`, `decomp` | `decompile <addr>` | Decompile function at address |
| `xrefs` | `x` | `xrefs <addr>` | Show cross-references to address |
| `clear` | `cls` | `clear` | Clear the screen |
| `help` | `?`, `h` | `help` | Show available commands |
| `quit` | `exit`, `q` | `quit` | Exit the program |

**Direct Analysis Flags** (skip REPL, run once and exit):
- `--info` - Display binary information and exit
- `--sections` - Show section table and exit
- `--strings` - Extract strings and exit
- `--xrefs <addr>` - Show cross-references and exit
- `--count` - Show counts of functions, strings, sections, and imports

**Address Formats Supported:**
- Hexadecimal: `0x1000`, `0x140001000`
- Decimal: `4096`, `5368713216`
- Without prefix: `1000` (interpreted as hex if valid)

### CLI Examples

```bash
# Load a binary in REPL mode
$ fission --cli /path/to/binary.exe
fission> info
Format:      PE64
Architecture: x86_64
Entry Point: 0x140001000
Sections:    5

# Direct analysis (one-shot commands)
$ fission --cli binary.exe --info
Format:      PE64
Architecture: x86_64
Entry Point: 0x140001000
...

$ fission --cli binary.exe --sections
[.text]     0x1000 - 0x5000 (RX)
[.rdata]    0x6000 - 0x8000 (R)
[.data]     0x9000 - 0xA000 (RW)
...

$ fission --cli binary.exe --strings
[0x402000] "Hello World"
[0x402010] "config.txt"
[0x402020] "Error: Failed to initialize"
...

$ fission --cli binary.exe --xrefs 0x140001234
References to 0x140001234:
  0x140001100: call 0x140001234
  0x140001500: jmp 0x140001234
...

# REPL mode commands
fission> funcs
[0x140001000] entry
[0x140001234] sub_140001234
[0x140001500] malloc
...

fission> disasm 0x140001000 20
0x140001000: push rbp
0x140001001: mov rbp, rsp
0x140001004: sub rsp, 0x20
...

fission> decompile 0x140001234
int sub_140001234(void) {
    HANDLE hFile;
    hFile = CreateFileA("config.txt", GENERIC_READ, 0, NULL, OPEN_EXISTING, 0, NULL);
    ...
}
```

---

## Configuration

### Configuration File

Fission stores its configuration in `~/.config/fission/config.toml` (Linux/macOS) or `%APPDATA%\fission\config.toml` (Windows).

```toml
# Example configuration file

[decompiler]
mode = "ffi"            # Direct FFI integration (default)
timeout_ms = 30000      # Decompilation timeout in milliseconds
enable_prefetch = true  # Pre-decompile adjacent functions
prefetch_count = 3      # Number of functions to prefetch
enable_optimizer = true # Enable Pcode IR optimizer (Phase 1 + Phase 2 rules)
optimizer_max_passes = 10  # Maximum optimization iterations

[analysis]
max_string_length = 262144   # Max bytes to search for strings
min_string_length = 4        # Minimum string length
auto_xref_analysis = true    # Auto cross-reference on load
cache_size = 100             # LRU cache entries

[debug]
max_snapshots = 10000        # TTD snapshot limit
max_process_ids = 4096       # Process enumeration limit

[ui]
theme = "catppuccin"         # Theme name
show_performance = false     # Show performance metrics
auto_scroll_entry = true     # Auto-scroll to entry on load
max_log_entries = 1000       # Console history limit
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `FISSION_DECOMP_PATH` | Path to `fission_decomp` binary | `./ghidra_decompiler/build/fission_decomp` |
| `FISSION_CONFIG_DIR` | Configuration directory | `~/.config/fission` |
| `FISSION_LOG_LEVEL` | Log level (error, warn, info, debug, trace) | `info` |
| `FISSION_CACHE_DIR` | Cache directory for temporary files | `~/.cache/fission` |

### Decompiler Settings

```rust
// Settings in src/core/config.rs

DecompilerConfig {
    mode: DecompilerMode::Pool,   // Single (memory efficient) or Pool (parallel)
    num_workers: 0,               // 0 = auto (CPU cores, max 8)
    max_workers: 8,               // Cap on auto-detected workers
    default_function_size: 4096,  // 4KB default function size
    max_function_size: 65536,     // 64KB max function size
    min_function_size: 16,        // 16 bytes minimum
    timeout_ms: 30000,            // 30 second timeout
    enable_prefetch: true,        // Pre-decompile adjacent functions
    prefetch_count: 3,            // Number of functions to prefetch
    requests_before_restart: 500, // Restart subprocess to reclaim memory
}

AnalysisConfig {
    max_string_search_size: 262144, // 256KB for string extraction
    min_string_length: 4,           // Minimum 4 bytes for strings
    auto_xref_analysis: true,       // Auto cross-reference on load
    decompile_cache_size: 100,      // LRU cache entries
    function_address_range: 4096,   // 4KB range for function matching
}

DebugConfig {
    max_snapshots: 10000,   // Time travel debugging snapshots
    max_process_ids: 4096,  // Max processes to enumerate
}

UiConfig {
    show_performance: false,   // Performance metrics display
    auto_scroll_entry: true,   // Auto-scroll to entry point on load
    max_log_entries: 1000,     // Console history limit
    hex_rows_per_page: 64,     // Hex viewer pagination
}
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         Fission (Rust)                           │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │  GUI (egui)  │  │  CLI (repl)  │  │   Plugin Manager       │  │
│  └──────┬───────┘  └──────┬───────┘  └───────────┬────────────┘  │
│         │                 │                      │               │
│         └─────────────────┴──────────────────────┘               │
│                           │                                       │
│  ┌────────────────────────┴────────────────────────────────────┐ │
│  │                    Analysis Core                             │ │
│  │  ┌─────────┐  ┌──────────┐  ┌─────────┐  ┌───────────────┐  │ │
│  │  │ Loader  │  │ Disasm   │  │ Decomp  │  │ Debug Engine  │  │ │
│  │  │ PE/ELF  │  │ iced-x86 │  │  FFI    │  │ Win32/ptrace  │  │ │
│  │  └─────────┘  └──────────┘  └────┬────┘  └───────────────┘  │ │
│  └──────────────────────────────────┼──────────────────────────┘ │
└─────────────────────────────────────┼────────────────────────────┘
                                      │ Direct FFI (zero-copy)
              ┌───────────────────────┴───────────────────────┐
              │          Ghidra Engine (C++)                  │
              │  SleighArch → Funcdata → Pcode Optimizer →   │
              │  → PrintC → C Code (via CXX bridge)          │
              └───────────────────────────────────────────────┘
```

### Component Overview

| Component | Description |
|-----------|-------------|
| **GUI (egui)** | Immediate-mode GUI with VS Code-inspired layout |
| **CLI (reedline)** | Interactive REPL with command history and completion |
| **Plugin Manager** | Loads and manages Rust/Python plugins |
| **Loader** | Parses PE, ELF, and Mach-O binary formats |
| **Disasm** | x86/x64 disassembly using iced-x86 |
| **Decomp FFI** | Direct C++ integration via CXX bridge (zero IPC overhead) |
| **Pcode Optimizer** | 32+ optimization rules with def-use tracking and NZMask analysis |
| **Debug Engine** | Platform-specific debugging (Win32/ptrace) |
| **Ghidra Engine** | Native C++ decompiler with optimized Pcode IR |

### Decompilation Pipeline

```
Binary → SleighArch → Raw Pcode → Optimizer (32+ rules) → Optimized Pcode
  ↓                                    ↓
Entry        Phase 1: Constant folding, algebraic simplification
Point        Phase 2: Def-use tracking, NZMask, shift-bitops, AND-mask
  ↓                                    ↓
Funcdata ← ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ↓
  ↓
PrintC → C Code (optimized, with type info)
```

---

## Plugin Development

### Rust Plugins

Create a new Rust plugin by implementing the `FissionPlugin` trait:

```rust
use fission::plugin::{FissionPlugin, PluginInfo, HookPriority};
use fission::core::events::{Event, EventResult};

pub struct MyPlugin {
    name: String,
}

impl FissionPlugin for MyPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "My Plugin".to_string(),
            version: "1.0.0".to_string(),
            author: "Your Name".to_string(),
            description: "A custom analysis plugin".to_string(),
        }
    }

    fn on_load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Plugin loaded!");
        Ok(())
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::BinaryLoaded { path, .. } => {
                println!("Binary loaded: {}", path);
                EventResult::Continue
            }
            Event::FunctionDecompiled { address, code, .. } => {
                println!("Decompiled 0x{:x}", address);
                // Analyze the decompiled code
                EventResult::Continue
            }
            _ => EventResult::Continue,
        }
    }

    fn priority(&self) -> HookPriority {
        HookPriority::Normal
    }
}
```

### Python Scripting

Fission exposes a Python API via PyO3 when compiled with the `python` feature:

```python
import fission

# Load a binary
binary = fission.load("/path/to/binary.exe")

# Get binary information
print(f"Format: {binary.format}")
print(f"Architecture: {binary.arch}")
print(f"Entry Point: 0x{binary.entry_point:x}")

# List functions
for func in binary.functions():
    print(f"[0x{func.address:x}] {func.name}")

# Get sections
for section in binary.sections():
    print(f"{section.name}: 0x{section.address:x} ({section.size} bytes)")

# Disassemble at address
for insn in binary.disassemble(0x140001000, count=10):
    print(f"0x{insn.address:x}: {insn.mnemonic} {insn.operands}")

# Decompile a function
code = binary.decompile(0x140001000)
print(code)

# Access strings
for s in binary.strings():
    print(f"0x{s.address:x}: {s.value}")
```

### Event Types

| Event | Description | Data |
|-------|-------------|------|
| `BinaryLoaded` | Fired when a binary is loaded | path, format, arch |
| `FunctionDecompiled` | Fired after decompilation | address, code, function_name |
| `BreakpointHit` | Fired when a breakpoint triggers | address, hit_count |
| `DebugEvent` | General debug events | event_type, data |
| `Custom` | User-defined events | name, payload |

---

## Tech Stack

| Component | Technology | Version | Purpose |
|-----------|------------|---------|---------|
| **Core Language** | Rust | 2021 Edition | Performance and safety |
| **GUI Framework** | egui + eframe | 0.29 | Immediate-mode UI |
| **Theme** | Catppuccin | - | Soothing color palette |
| **Disassembler** | iced-x86 | 1.21 | Pure Rust x86/x64 disassembly |
| **Decompiler** | Ghidra (C++) | - | C code generation |
| **PE/ELF Parsing** | goblin + object | 0.8 / 0.32 | Binary format parsing |
| **.NET Parsing** | Custom Rust | - | CLR metadata and IL |
| **Windows Debug** | windows crate | 0.54 | Win32 debug API |
| **Linux Debug** | nix | 0.28 | ptrace interface |
| **Scripting** | PyO3 | 0.24 | Python bindings |
| **Async Runtime** | Tokio | 1.36 | Async I/O and threading |
| **Caching** | lru | 0.12 | LRU result cache |
| **CLI** | reedline + clap | 0.30 / 4.5 | Command-line interface |
| **Serialization** | serde + serde_json | 1.0 | Data serialization |

---

## Performance

### Benchmark Results

> Note: Benchmarks performed on AMD Ryzen 9 5900X, 32GB RAM, NVMe SSD

| Operation | Single Mode | Pool Mode (8 workers) |
|-----------|-------------|----------------------|
| Load PE (1MB) | 12ms | 12ms |
| Function Discovery | 45ms | 45ms |
| Decompile (small func) | 150ms | 150ms |
| Decompile (100 funcs) | 15s | 2.1s |
| String Extraction | 8ms | 8ms |

### Optimization Tips

1. **Use Pool Mode for Large Binaries**
   ```toml
   [decompiler]
   mode = "pool"
   num_workers = 0  # Auto-detect
   ```

2. **Increase Cache Size for Repeated Analysis**
   ```toml
   [analysis]
   cache_size = 500  # More cache entries
   ```

3. **Enable Prefetching for Sequential Navigation**
   ```toml
   [decompiler]
   enable_prefetch = true
   prefetch_count = 5
   ```

4. **Reduce Memory Usage**
   ```toml
   [decompiler]
   mode = "single"  # Single process mode
   requests_before_restart = 100  # Restart more frequently
   ```

---

## Comparison with Other Tools

| Feature | Fission | Ghidra | IDA Pro | x64dbg | radare2 |
|---------|---------|--------|---------|--------|---------|
| **Price** | Free | Free | $$$$ | Free | Free |
| **Decompiler** | Yes (Ghidra) | Yes | Yes | No | Yes (r2ghidra) |
| **Debugger** | Yes | Yes | Yes | Yes | Yes |
| **Time-Travel Debug** | Yes | No | Yes (paid) | No | No |
| **GUI** | Modern | Java | Native | Native | Web/TUI |
| **Scripting** | Python/Rust | Java/Python | Python/IDC | Plugin | r2pipe |
| **Cross-Platform** | Yes | Yes | Yes | Windows | Yes |
| **Performance** | Fast (Rust) | Moderate | Fast | Fast | Fast |
| **.NET Support** | Yes | Plugin | Yes | No | Limited |
| **Unified Tool** | Yes | No | Partial | No | Yes |

### When to Use Fission

- **Single tool for everything**: Want static and dynamic analysis in one place
- **Performance matters**: Need fast analysis of large binaries
- **Modern UX**: Prefer VS Code-style interface over older designs
- **Extensibility**: Want to write plugins in Rust or Python
- **Free and Open Source**: Need a capable tool without licensing costs

### When to Use Others

- **IDA Pro**: Need the most mature decompiler for complex binaries
- **Ghidra**: Want extensive collaboration features and scripting
- **x64dbg**: Need advanced Windows debugging features
- **radare2**: Prefer command-line workflow with scripting

---

## Project Structure

```
Fission/
├── Cargo.toml                 # Rust package manifest
├── Cargo.lock                 # Dependency lock file
├── build.rs                   # Build script for native library linking
├── README.md                  # This file
├── LICENSE                    # MIT License
│
├── ghidra_decompiler/         # Native C++ decompiler
│   ├── CMakeLists.txt         # CMake build configuration
│   ├── fission_decomp.cpp     # Main decompiler subprocess
│   ├── src/                   # C++ source files
│   └── languages/             # Sleigh (.sla/.sinc) instruction definitions
│
├── src/
│   ├── main.rs                # CLI entry point with mode switching
│   ├── lib.rs                 # Library root exports
│   │
│   ├── core/                  # Fundamental utilities
│   │   ├── config.rs          # All configuration options
│   │   ├── context.rs         # FissionContext (app-wide state)
│   │   ├── events.rs          # Event bus system
│   │   ├── errors.rs          # Unified error types
│   │   ├── logging.rs         # Log levels and file output
│   │   ├── constants.rs       # Magic bytes and offsets
│   │   └── modules.rs         # Module lifecycle
│   │
│   ├── analysis/              # Static analysis
│   │   ├── loader/            # PE/ELF/Mach-O parsing
│   │   ├── disasm/            # iced-x86 wrapper
│   │   ├── decomp/            # Decompiler pool management
│   │   ├── dotnet/            # .NET/CLR analysis
│   │   ├── signatures/        # Windows API mappings
│   │   ├── xrefs/             # Cross-reference database
│   │   ├── detector/          # Binary signature detection
│   │   ├── patch/             # Memory patching
│   │   └── gdt_parser.rs      # GDT type extraction
│   │
│   ├── debug/                 # Dynamic analysis
│   │   ├── windows/           # Win32 debugger
│   │   ├── linux.rs           # ptrace debugger
│   │   ├── ttd/               # Time travel debugging
│   │   ├── traits.rs          # Platform-agnostic trait
│   │   └── memory.rs          # Cross-platform memory ops
│   │
│   ├── plugin/                # Extension system
│   │   ├── traits.rs          # FissionPlugin trait
│   │   ├── manager.rs         # Plugin registry
│   │   ├── python.rs          # PyO3 integration
│   │   ├── hooks.rs           # Event hooks
│   │   └── api.rs             # Exported plugin API
│   │
│   ├── script/                # Scripting support
│   │   ├── bridge.rs          # Python interoperability
│   │   └── types.rs           # Exported Python types
│   │
│   └── ui/                    # User interface
│       ├── gui/               # egui-based GUI
│       │   ├── app/           # FissionApp orchestrator
│       │   ├── panels/        # UI components
│       │   ├── theme.rs       # Catppuccin styling
│       │   └── state.rs       # AppState management
│       └── cli/               # Command-line interface
│           ├── mod.rs         # REPL loop
│           └── commands.rs    # Command parsing
│
├── tests/                     # Integration tests
│   ├── cli_tests.rs
│   ├── decompiler_tests.rs
│   ├── loader_tests.rs
│   └── advanced_tests.rs
│
├── benches/                   # Performance benchmarks
│   └── benchmark.rs
│
├── scripts/                   # Build utilities
│   └── build_decompiler.sh
│
└── .github/workflows/         # CI/CD pipelines
    ├── ci.yml
    ├── security.yml
    ├── cd.yml
    └── ...
```

---

## Development Status

### Completed Features

- [x] **CLI Base** - Binary loader, disassembler, REPL interface
- [x] **Ghidra Integration** - Direct FFI integration via CXX bridge (zero-copy)
- [x] **Pcode IR Optimizer** - Phase 1 (30+ rules) + Phase 2 (def-use tracking, NZMask analysis)
- [x] **VS Code Style GUI** - Tabs, Activity Bar, Catppuccin theme
- [x] **.NET Support** - CLR detection, metadata parsing, IL disassembly
- [x] **Debugging** - Attach, breakpoints, registers, memory access
- [x] **Plugin System** - Native Rust and Python plugin support
- [x] **Performance Optimization** - Direct FFI, LRU caching, prefetch, optimizer
- [x] **Advanced Type Analysis** - Struct inference, VTable, type propagation
- [x] **Cross-Reference Analysis** - Code and data xref detection
- [x] **Smart Constant Substitution** - Windows API parameter mapping

### Recent Updates (January 2026)

- ✅ **Decompiler Architecture** - Migrated from subprocess pool to direct FFI integration
- ✅ **Pcode Optimizer Phase 1** - 30+ optimization rules (constant folding, algebraic simplification, dead code elimination)
- ✅ **Pcode Optimizer Phase 2** - Def-use tracking, NZMask analysis, RuleShiftBitops, RuleAndMask
- ✅ **Zero-Copy Integration** - Eliminated IPC overhead with native C++ bindings

### In Progress

- [ ] **Pcode Optimizer Phase 3** - CSE, RulePullSubIndirect, pointer arithmetic optimizations
- [ ] **Advanced TTD** - Full time travel debugging with complete state replay
- [ ] **Remote Debugging** - Network-based debug sessions

---

## Implementation Notes

The following items are either not fully implemented yet or need explicit verification:

- **Multi-process decompiler pool**: The README mentions a pool, but the current implementation uses in-process FFI; the pool is deprecated.
- **macOS debugging**: Attach/breakpoints/memory access are stubbed and require entitlements; only process listing is functional.
- **TTD replay**: Timeline/recording exists, but full deterministic replay across platforms is not yet verified (see Advanced TTD in progress).

---

## Roadmap

### Ultimate Goal: Project Restoration

> Transform binaries back into original source projects. Even with different variable names and ordering, if functionality is identical, the programs are equivalent.

### Three AI Agents (Future)

| Agent | Role | Technology |
|-------|------|------------|
| **Observer** | Static Analysis | Decompilation, type inference, data flow, pattern recognition |
| **Executor** | Dynamic Analysis | Runtime tracing, memory snapshots, I/O monitoring, coverage |
| **Author** | Code Generation | Inference-verification-correction loop, test generation, build verification |

### Planned Phases

1. **AI Integration** - LLM API connection (OpenAI, Claude, Local models)
2. **Dynamic Analysis AI** - Execution trace analysis and I/O pattern detection
3. **Code Generation** - AI-powered source code reconstruction
4. **Full Restoration** - Complete project recovery including build systems

---

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test loader_tests
cargo test --test decompiler_tests
cargo test --test cli_tests
cargo test --test advanced_tests

# Run with verbose output
cargo test -- --nocapture

# Run tests with specific name pattern
cargo test function_discovery

# Run benchmarks
cargo bench
```

### Test Coverage

```bash
# Using cargo-tarpaulin
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Continuous Integration

The project uses GitHub Actions for CI/CD with the following pipelines:

| Pipeline | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | Push/PR | Build and test on Ubuntu/macOS/Windows |
| `security.yml` | Push/PR | CodeQL analysis and Trivy scanning |
| `audit.yml` | Daily/Push | Dependency security audit |
| `benchmark.yml` | Push to main | Performance regression detection |
| `coverage.yml` | Push/PR | Code coverage tracking |
| `fuzz.yml` | Weekly | Fuzzing for crash detection |

---

## Troubleshooting

### Build Issues

#### CMake not finding ZLIB (Windows)

```powershell
# Ensure vcpkg is installed and ZLIB is available
C:\vcpkg\vcpkg install zlib:x64-windows

# Set the toolchain file
cmake -B build -DCMAKE_TOOLCHAIN_FILE=C:\vcpkg\scripts\buildsystems\vcpkg.cmake
```

#### Linker errors on Linux

```bash
# Install missing development libraries
sudo apt install libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libxkbcommon-dev
```

#### Python feature build fails

```bash
# Ensure Python development headers are installed
# Ubuntu/Debian
sudo apt install python3-dev

# macOS
brew install python

# Check PyO3 requirements
pip install maturin
```

### Runtime Issues

#### Decompiler not found

```bash
# Check if fission_decomp exists
ls -la ghidra_decompiler/build/fission_decomp

# Set the path explicitly
export FISSION_DECOMP_PATH=/path/to/fission_decomp
```

#### Decompilation timeout

```toml
# Increase timeout in config.toml
[decompiler]
timeout_ms = 60000  # 60 seconds
```

#### High memory usage

```toml
# Use single mode and restart workers frequently
[decompiler]
mode = "single"
requests_before_restart = 50
```

#### GUI not starting

```bash
# Check for display issues (Linux)
echo $DISPLAY

# Try software rendering
export LIBGL_ALWAYS_SOFTWARE=1
cargo run --release
```

### Debug Issues

#### Cannot attach to process (Windows)

- Run Fission as Administrator
- Check if the target process is protected
- Disable anti-debugging in target if legitimate

#### ptrace fails (Linux)

```bash
# Check ptrace permissions
cat /proc/sys/kernel/yama/ptrace_scope

# Temporarily allow (requires root)
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

# Or run as root
sudo ./target/release/fission
```

---

## FAQ

### General

**Q: Is Fission free to use?**
A: Yes, Fission is free and open source under the MIT license. You can use it for personal, educational, and commercial purposes.

**Q: Which platforms are supported?**
A: Fission runs on Windows, Linux, and macOS. It can analyze binaries from all three platforms regardless of the host OS.

**Q: Do I need Ghidra installed?**
A: No. Fission includes its own native decompiler built from Ghidra's open-source components. You don't need to install Ghidra separately.

### Technical

**Q: Why is decompilation slow?**
A: First-time decompilation requires architecture initialization. Subsequent decompilations use cached objects and are much faster. Enable pool mode for parallel processing.

**Q: Can I analyze packed/obfuscated binaries?**
A: Fission can load and disassemble packed binaries, but decompilation may produce poor results. Use the debugger to unpack at runtime, then analyze the unpacked code.

**Q: How do I add support for a new binary format?**
A: Implement the `BinaryLoader` trait in `src/analysis/loader/` and register it in the loader factory. See existing PE/ELF implementations for reference.

**Q: Can I use Fission for malware analysis?**
A: Yes, Fission is designed with malware analysis in mind. Use appropriate isolation (VMs, sandboxes) when analyzing potentially malicious samples.

### Comparison

**Q: How does Fission compare to IDA Pro?**
A: IDA Pro has a more mature decompiler and larger plugin ecosystem. Fission offers a modern UI, is completely free, and provides integrated debugging with time-travel capabilities.

**Q: Should I use Fission or Ghidra?**
A: Ghidra is excellent for collaborative analysis and has more features. Fission offers better performance, a more modern UI, and integrated debugging in a single tool.

---

## Contributing

We welcome contributions! Here's how to get started:

### Development Setup

```bash
# Fork and clone the repository
git clone https://github.com/YOUR_USERNAME/Fission.git
cd Fission

# Create a feature branch
git checkout -b feature/your-feature-name

# Build in debug mode for faster compilation
cargo build

# Run tests
cargo test
```

### Code Style

- Follow Rust standard formatting: `cargo fmt`
- Pass all clippy lints: `cargo clippy -- -D warnings`
- Add tests for new functionality
- Update documentation for API changes

### Pull Request Process

1. Ensure all tests pass
2. Update README.md if adding user-facing features
3. Add a clear description of changes
4. Reference any related issues
5. Request review from maintainers

### Areas Needing Help

- [ ] Additional binary format support (Mach-O improvements)
- [ ] More Windows API signatures
- [ ] Plugin examples and documentation
- [ ] Performance optimizations
- [ ] GUI improvements and accessibility

---

## Security

### Reporting Vulnerabilities

If you discover a security vulnerability, please:

1. **Do NOT** open a public issue
2. Email security concerns to the maintainers
3. Include detailed reproduction steps
4. Allow reasonable time for a fix before disclosure

### Security Features

- Sandboxed subprocess execution for decompilation
- Input validation for binary parsing
- Memory-safe Rust implementation
- Regular dependency auditing via `cargo audit`

### Security Considerations

When analyzing untrusted binaries:

- Use a virtual machine or isolated environment
- Be cautious with dynamic analysis features
- The decompiler runs in a subprocess to limit potential exploits

---

## License

MIT License - See [LICENSE](LICENSE) for details.

```
MIT License

Copyright (c) 2024 Fission Dev Team

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Acknowledgments

Fission would not be possible without these amazing open source projects:

- [**Ghidra**](https://ghidra-sre.org/) - NSA's software reverse engineering framework, providing the decompiler engine
- [**iced-x86**](https://github.com/icedland/iced) - High-performance x86/x64 disassembler written in Rust
- [**egui**](https://github.com/emilk/egui) - Easy-to-use immediate mode GUI library for Rust
- [**Catppuccin**](https://github.com/catppuccin/catppuccin) - Soothing pastel theme for the UI
- [**goblin**](https://github.com/m4b/goblin) - Cross-platform binary parsing library
- [**tokio**](https://tokio.rs/) - Asynchronous runtime for Rust
- [**PyO3**](https://pyo3.rs/) - Rust bindings for Python

Special thanks to all contributors and the reverse engineering community for their feedback and support.

---

## Documentation

For detailed information about Fission:

- **[Architecture Guide](docs/architecture/ARCHITECTURE.md)** - Module structure, design principles, analysis pipeline
- **[Build Guide](docs/build/BUILD.md)** - Build instructions and dependencies
- **[GUI Guide](docs/gui/GUI_GUIDE.md)** - UI usage and workflows
- **[CLI One-shot Mode](docs/cli/CLI_ONE_SHOT_MODE.md)** - CLI usage for batch/oneshot analysis
- **[Decompiler Comparison](docs/decompiler/DECOMPILER_COMPARISON.md)** - Ghidra vs Fission comparison workflow
- **[Plugin Development](docs/plugins/PLUGIN_DEVELOPMENT.md)** - Extend Fission with plugins
- **[Constant Substitution](docs/analysis/CONSTANT_SUBSTITUTION.md)** - Analysis details
- **[GCC FID Implementation](docs/analysis/GCC_FID_IMPLEMENTATION.md)** - Function ID implementation notes

---

<p align="center">
  Made with Rust
</p>
