# Fission

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Security](https://github.com/sjkim1127/Fission/actions/workflows/security.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)]()

> **"Split the Binary, Fuse the Power."**

A next-generation binary analysis platform unifying decompilation, disassembly, dynamic debugging,
and time-travel debugging in a single high-performance Rust-powered tool.

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Decompiler Quality](#decompiler-quality)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [GUI — Tauri Desktop App](#gui--tauri-desktop-app)
- [CLI Reference](#cli-reference)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Plugin Development](#plugin-development)
- [Tech Stack](#tech-stack)
- [Comparison with Other Tools](#comparison-with-other-tools)
- [Project Structure](#project-structure)
- [Development Status](#development-status)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgments](#acknowledgments)

---

## Overview

**Fission** is a comprehensive reverse engineering platform built in Rust, combining:

- **Static Analysis** — Ghidra-powered decompiler via direct C++ FFI, pure-Rust disassembler, CFG analysis, RTTI recovery
- **Dynamic Analysis** — Process debugging, breakpoints, memory inspection, time-travel debugging (Windows)
- **Smart Post-Processing** — 32+ Pcode IR optimization rules, context-aware constant substitution (100+ Windows API mappings), type propagation, x86 `double` synthesis

### Why Fission?

| | |
|-|-|
| **Decompiler quality** | x64 **98.8%** · x86 **92.6%** vs Ghidra baseline |
| **Integration** | Static + dynamic analysis in one binary |
| **Performance** | Rust core, zero-copy Ghidra FFI, LRU cache |
| **GUI** | Tauri 2.x + React 19 desktop app |
| **Platforms** | Windows PE · Linux ELF · macOS Mach-O |
| **Extensible** | Native Rust plugin system |

---

## Features

### Static Analysis

| Feature | Description |
|---------|-------------|
| **Ghidra-Powered Decompiler** | Zero-copy C decompilation via direct C++ FFI |
| **iced-x86 Disassembler** | Pure Rust x86/x64 disassembly with syntax highlighting |
| **Cross-Platform Loaders** | Windows PE, Linux ELF, macOS Mach-O |
| **Cross-Reference Analysis** | Code and data xref detection |
| **String Extraction** | ASCII + UTF-16 LE scanning with context |
| **CFG Analysis** | Dominator tree, loop detection, cyclomatic complexity |
| **Listing View** | Full binary disassembly with virtual scrolling |
| **C++ RTTI Recovery** | VTable parsing, type_descriptor, virtual dispatch resolution |

### Smart Decompilation Post-Processing

| Feature | Description |
|---------|-------------|
| **Context-Aware Constant Substitution** | PAGE_PROTECT, MEM_ALLOC, GENERIC_ACCESS, etc. |
| **100+ Windows API Mappings** | 9 DLLs: kernel32, user32, ntdll, advapi32, ws2_32, winhttp, wininet, shell32, bcrypt |
| **Dynamic Flag Resolution** | e.g., `0x3000` → `MEM_COMMIT | MEM_RESERVE` |
| **GDT Type Loading** | 5,700+ structures and 6,500+ typedefs from Ghidra data |
| **Pcode IR Optimizer** | 32+ rules: constant folding, CSE, DCE, pointer arithmetic, NZMask |
| **x86 Double Synthesis** | Merges two 4-byte cdecl stack pushes into a single `double` |
| **Type Propagation** | Propagates callee type info back to callers |
| **Smart String Recovery** | Converts hex constants into readable string literals |

### Dynamic Analysis

| Feature | Description |
|---------|-------------|
| **Process Debugging** | Attach/detach with register & memory access |
| **Breakpoints** | Software breakpoints with hit counting |
| **Time Travel Debugging** | Execution timeline + snapshot navigation (Windows) |
| **Live Memory Patching** | Modify running process memory in real-time |

### GUI (Tauri 2.x + React 19)

| Feature | Description |
|---------|-------------|
| **VS Code-Style Layout** | Activity Bar, tabbed editor, bottom panel |
| **Virtual Scrolling** | `@tanstack/react-virtual` — 5,000+ assembly lines |
| **CFG Visualization** | SVG-rendered CFG with pan/zoom and UI scale slider |
| **Function Explorer** | Filter (All / Imports / Exports / Internal), search, FID IDs |
| **String XRefs** | UTF-16 LE aware, virtual scroll, click-to-navigate |
| **Debug Panel** | Registers, memory hex dump (up to 4 KB), TTD timeline |
| **Analysis Export** | JSON export of full analysis results |
| **Project Save/Load** | `.fprj` JSON project files |

---

## Decompiler Quality

Measured by `scripts/compare/compare_decompilers_v3.py` against Ghidra 11.x on Windows PE binaries.

### x64 — **98.8%** (Feb 2026)

| Function | Score |
|----------|-------|
| `add` | 100% |
| `multiply` | 100% |
| `print_message` | 100% |
| `init_item` | 100% |
| `create_item` | 100% |
| `calculate_discount` | 100% |
| `sum_array` | 94.4% |
| `main` | 96.6% |
| **Average** | **98.8%** |

### x86 MinGW (-O1) — **92.6%** (Feb 2026)

| Function | Score |
|----------|-------|
| `add` | 100% |
| `multiply` | 100% |
| `print_message` | 100% |
| `init_item` | 100% |
| `create_item` | 100% |
| `calculate_discount` | 92.9% |
| `sum_array` | 88.2% |
| `main` | 60.0% |
| **Average** | **92.6%** |

**Milestone history:**

| Date | x64 | x86 | Key Change |
|------|-----|-----|------------|
| 2026-02-15 | 98.8% | — | MinGW TypePropagator, integer-cast stripping |
| 2026-02-14 | — | 80.0% | x86 benchmark suite created |
| 2026-02-24 | — | 90.1% | Track 2/3/4 + Normalizer A-1~A-6 |
| **2026-02-25** | — | **92.6%** | x86 double synthesis + VAR normalization fix |

---

## Installation

### Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust | 1.85+ | [rustup.rs](https://rustup.rs/) |
| CMake | 3.16+ | Ghidra decompiler build |
| C++ Compiler | GCC 12+ / Clang 15+ / MSVC 2022 | Platform-specific |
| Node.js | 20+ | Tauri GUI only |
| vcpkg | Latest | Windows only (ZLIB) |

### macOS

```bash
xcode-select --install
brew install cmake pkg-config zlib node

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

git clone https://github.com/sjkim1127/Fission.git && cd Fission

cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
cd ..

cargo build --release --bin fission_cli
```

### Linux (Ubuntu/Debian)

```bash
sudo apt install -y build-essential cmake pkg-config zlib1g-dev libssl-dev \
    libgtk-3-dev libwebkit2gtk-4.1-dev nodejs npm

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

git clone https://github.com/sjkim1127/Fission.git && cd Fission
cd ghidra_decompiler && cmake -B build && cmake --build build && cd ..
cargo build --release --bin fission_cli
```

### Windows

```powershell
winget install Rustlang.Rustup Kitware.CMake OpenJS.NodeJS

git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install zlib:x64-windows
$env:VCPKG_ROOT = "C:\vcpkg"

git clone https://github.com/sjkim1127/Fission.git; cd Fission
cd ghidra_decompiler
cmake -B build -DCMAKE_TOOLCHAIN_FILE=C:\vcpkg\scripts\buildsystems\vcpkg.cmake
cmake --build build --config Release
cd ..
cargo build --release --bin fission_cli
```

### Feature Flags

```bash
cargo build --release --bin fission_cli              # CLI only
cargo build --release --bin fission_cli --features tui  # + terminal UI
cargo build --release --features "cli tui native_decomp" # all
```

---

## Quick Start

### CLI

```bash
# Decompile a function (add --verbose to see C++ debug output)
fission_cli --decomp 0x401000 binary.exe
fission_cli --decomp 0x401000 --verbose binary.exe

# Other quick flags
fission_cli --info binary.exe
fission_cli --funcs binary.exe
fission_cli --strings binary.exe
fission_cli --cfg 0x401000 --cfg-format dot -o cfg.dot binary.exe

# Interactive REPL
fission_cli binary.exe
```

### GUI (Tauri)

```bash
cd crates/fission-tauri
npm install
npm run tauri dev    # dev (hot reload)
npm run tauri build  # production
```

1. **Open Binary**: File → Open (or drag-and-drop)
2. **Browse Functions**: Explorer sidebar → filter by category
3. **Decompile**: Click function → switch to **Decompiled** tab
4. **CFG**: Click **CFG** → pan/zoom; scale with right-side slider
5. **Analyze**: Toolbar → Analyze 🔍 (CALL scan) or Deep Scan 🕵
6. **Debug** (Windows): Attach process, set breakpoints, inspect registers, TTD timeline
7. **Export**: File → Export Analysis JSON

---

## GUI — Tauri Desktop App

Built with **Tauri 2.x** (Rust backend) + **React 19 / TypeScript** frontend.
The previous egui-based `fission-ui` crate has been fully removed.

### Key IPC Commands

| Command | Description |
|---------|-------------|
| `open_file` | Load binary |
| `decompile_function` | Decompile at address |
| `get_cfg` | CFG JSON for SVG render |
| `get_strings` | List strings |
| `get_xrefs` | Cross-references |
| `analyze_functions` | CALL-scan function discovery |
| `deep_scan_functions` | Prologue-pattern discovery |
| `run_fid` | FID signature identification |
| `debug_attach / detach` | Process attach/detach |
| `debug_read_memory` | Hex dump up to 4 KB |
| `export_analysis_json` | Full analysis JSON export |
| `save_project / load_project` | `.fprj` project persistence |

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Cmd/Ctrl + ←/→ | Cycle tabs |
| F5 | Analyze Functions |
| F6 | Deep Scan Functions |
| Cmd/Ctrl + O | Open Binary |
| Cmd/Ctrl + S | Save Project |

---

## CLI Reference

### REPL Commands

| Command | Aliases | Description |
|---------|---------|-------------|
| `load <path>` | `open`, `o` | Load a binary |
| `info` | `i` | Format, arch, entry point |
| `funcs` | `functions`, `f` | List all functions |
| `sections` | `sec` | Section table |
| `strings` | `str` | Extract strings |
| `analyze` | `anal`, `a` | Function discovery |
| `disasm <addr> [n]` | `dis`, `d` | Disassemble |
| `decompile <addr>` | `dec`, `decomp` | Decompile function |
| `xrefs <addr>` | `x` | Cross-references |
| `help` | `?`, `h` | Show commands |
| `quit` | `exit`, `q` | Exit |

### Direct Flags

| Flag | Description |
|------|-------------|
| `--info` | Binary info |
| `--sections` | Section table |
| `--strings` | String extraction |
| `--funcs` | Function list |
| `--xrefs <addr>` | Cross-references |
| `--decomp <addr>` | Decompile function |
| `--cfg <addr>` | CFG analysis |
| `--cfg-format dot\|json` | CFG output format |
| `--no-header` | Suppress function banner |
| `--verbose` | Show C++ debug output |

---

## Configuration

`~/.config/fission/config.toml` (Linux/macOS) or `%APPDATA%\fission\config.toml` (Windows):

```toml
[decompiler]
timeout_ms = 30000
enable_prefetch = true
prefetch_count = 3
enable_optimizer = true
optimizer_max_passes = 10

[analysis]
min_string_length = 4
auto_xref_analysis = true
cache_size = 100

[debug]
max_snapshots = 10000

[ui]
theme = "catppuccin"
auto_scroll_entry = true
max_log_entries = 1000
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FISSION_DECOMP_PATH` | `./ghidra_decompiler/build/fission_decomp` | Native decompiler path |
| `FISSION_SLA_DIR` | `ghidra_decompiler/languages` | Sleigh definitions |
| `FISSION_LOG_LEVEL` | `info` | error/warn/info/debug/trace |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Fission Platform                   │
├──────────────┬──────────────────────────────────────┤
│  Tauri GUI   │         fission_cli                  │
│ (React 19 /  │    (REPL + direct flags)              │
│  TypeScript) │                                      │
├──────────────┴──────────────────────────────────────┤
│              fission-analysis (Rust)                │
│  CFG · XRef · Debug · Plugin · FID · Type Prop      │
├─────────────────────────────────────────────────────┤
│  fission-ffi  │  fission-loader  │  fission-disasm  │
│  (C++ FFI)    │  (PE/ELF/Mach-O) │  (iced-x86)      │
├───────────────┴──────────────────┴──────────────────┤
│       ghidra_decompiler/  (C++, CMake)              │
│   Ghidra PCode Engine  ·  Sleigh ISA defs           │
│   TypePropagator  ·  AnalysisPipeline               │
│   PostProcessors  ·  CFGStructurizer                │
└─────────────────────────────────────────────────────┘
```

See [docs/architecture/](docs/architecture/) and [docs/RUST_CPP_BRIDGE.md](docs/RUST_CPP_BRIDGE.md).

---

## Plugin Development

```rust
use fission::plugin::{FissionPlugin, PluginContext};

pub struct MyPlugin;

impl FissionPlugin for MyPlugin {
    fn id(&self) -> &str { "my_plugin" }
    fn name(&self) -> &str { "My Plugin" }
    fn version(&self) -> &str { "0.1.0" }
    fn description(&self) -> &str { "Example" }

    fn on_binary_loaded(&self, _: &PluginContext, info: &fission::plugin::api::BinaryInfo) {
        println!("Loaded: {}", info.path);
    }
}

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn FissionPlugin {
    Box::into_raw(Box::new(MyPlugin))
}
#[no_mangle]
pub extern "C" fn destroy_plugin(p: *mut dyn FissionPlugin) {
    unsafe { drop(Box::from_raw(p)); }
}
```

Set `crate-type = ["cdylib"]` in `Cargo.toml`. See [docs/plugins/](docs/plugins/) for the full API.

---

## Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Core | Rust 2021 | Performance and safety |
| GUI Framework | Tauri 2.x + React 19 | Desktop app with web frontend |
| Disassembler | iced-x86 | Pure Rust x86/x64 |
| Decompiler | Ghidra (C++) | C code generation |
| Binary Parsing | goblin + object | PE / ELF / Mach-O |
| Windows Debug | windows crate | Win32 debug API |
| Linux Debug | nix (ptrace) | POSIX debug |
| Scripting | PyO3 | Python bindings |
| Async | Tokio | I/O and threading |
| CLI | reedline + clap | REPL and flags |
| Serialization | serde + serde_json | Data exchange |
| Virtual Scroll | @tanstack/react-virtual | GUI listing |

---

## Comparison with Other Tools

| Feature | Fission | Ghidra | IDA Pro | x64dbg | radare2 |
|---------|---------|--------|---------|--------|---------|
| Price | Free | Free | $$$$ | Free | Free |
| Decompiler | ✅ 98.8% / 92.6% | Baseline | High | ❌ | ✅ variable |
| Debugger | ✅ | ✅ | ✅ | ✅ | ✅ |
| Time-Travel Debug | ✅ (Windows) | ❌ | ✅ (paid) | ❌ | ❌ |
| GUI | Tauri/React | Java | Native | Native | Web/TUI |
| Cross-Platform | ✅ | ✅ | ✅ | Windows | ✅ |
| Scripting | Rust plugins | Java/Python | Python/IDC | C++ | r2pipe |

---

## Project Structure

```
Fission/
├── Cargo.toml
├── README.md
├── ghidra_decompiler/         # Native C++ decompiler layer
│   ├── CMakeLists.txt
│   ├── decompile/             # Upstream Ghidra core (do not modify)
│   ├── src/
│   │   ├── analysis/          # TypePropagator, PostProcessors, …
│   │   └── decompiler/        # AnalysisPipeline, DecompilationPipeline, …
│   └── languages/             # Sleigh (.sla/.sinc) ISA definitions
├── crates/
│   ├── fission-core/          # Config, errors, utilities
│   ├── fission-loader/        # Binary parsing
│   ├── fission-disasm/        # Disassembly abstraction
│   ├── fission-pcode/         # Pcode IR types
│   ├── fission-signatures/    # API/FID signature DBs
│   ├── fission-ffi/           # Rust ↔ C++ FFI boundary
│   ├── fission-analysis/      # CFG, XRef, debug, plugins (+ benches/)
│   ├── fission-tauri/         # Tauri desktop app (React + Rust backend)
│   └── fission-cli/           # CLI binary (fission_cli)
├── utils/signatures/          # GDT type DBs, DIE rules, FID sigs
├── tests/                     # Integration tests
├── scripts/
│   └── compare/               # Benchmark scripts & YAML suites
├── docs/
│   ├── architecture/
│   ├── analysis/
│   ├── changelog/CHANGELOG.md
│   └── …
└── examples/
    ├── comparison_test_x64
    └── binaries/
```

---

## Development Status

### Completed

- [x] CLI REPL + direct flag interface
- [x] Ghidra FFI (zero-copy direct C++ binding)
- [x] Pcode IR Optimizer (32+ rules, def-use tracking, NZMask)
- [x] Context-aware constant substitution (100+ Windows API mappings)
- [x] Type propagation + smart string recovery + VTable analysis
- [x] x86 `double` synthesis (`merge_split_double_args`)
- [x] CFG analysis (dominator tree, loop detection, cyclomatic complexity)
- [x] Listing view with virtual scrolling
- [x] C++ RTTI recovery
- [x] Tauri 2.x + React 19 GUI (30+ IPC commands)
- [x] FID signature identification
- [x] Time Travel Debugging (Windows)
- [x] Native Rust plugin system
- [x] Linear sweep function discovery for stripped PE

### Roadmap

- [ ] DWARF debug info import
- [ ] Python scripting (PyO3 API stabilization)  
- [ ] Windows ARM64 support
- [ ] DWARF-based variable naming
- [ ] Collaborative annotation sharing

---

## Testing

```bash
# Unit + integration tests
cargo test
cargo test --test decompiler_tests
cargo test --test cli_tests

# Benchmarks
cargo bench -p fission-analysis

# Decompiler quality benchmark
python3 scripts/compare/compare_decompilers_v3.py \
    --suite scripts/compare/suite_example.yaml   # x64
python3 scripts/compare/compare_decompilers_v3.py \
    --suite scripts/compare/suite_x86.yaml       # x86

# Build test binaries
bash scripts/build_all_tests.sh
```

---

## Troubleshooting

### C++ debug output not appearing

Pass `--verbose` — without it, `OutputSilencer` redirects stderr to `/dev/null`.

```bash
fission_cli --decomp 0x401000 --verbose binary.exe
```

### `libdecomp.dylib` / `decomp.dll` not found

Build the Ghidra decompiler:

```bash
cd ghidra_decompiler && cmake -B build && cmake --build build
```

Set `FISSION_DECOMP_PATH` if the binary is in a non-standard location.

### Tauri GUI build fails on Linux

```bash
sudo apt install libwebkit2gtk-4.1-dev
```

### x86 shows split `double` arguments

Ensure you are using the latest `ghidra_decompiler` build (commit `e80b18432` or newer) which
includes the `FuncCallSpecs`-based fix in `merge_split_double_args`.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Quick workflow:

```bash
git checkout -b feat/my-feature
# make changes
cargo test && cargo clippy
git commit -m "feat: describe change"
git push origin feat/my-feature
# open Pull Request
```

---

## License

MIT — see [LICENSE](LICENSE).

---

## Acknowledgments

- [Ghidra](https://ghidra-sre.org/) — NSA decompiler engine (Apache 2.0)
- [iced-x86](https://github.com/icedland/iced) — Pure Rust x86/x64 disassembler
- [Tauri](https://tauri.app/) — Desktop app framework
- [React](https://react.dev/) — Frontend framework
- [goblin](https://github.com/m4b/goblin) — Rust binary parsing
- [Tokio](https://tokio.rs/) — Async runtime
- [Catppuccin](https://catppuccin.com/) — Color palette

---

## Documentation

| Document | Description |
|----------|-------------|
| [docs/RUST_CPP_BRIDGE.md](docs/RUST_CPP_BRIDGE.md) | Rust ↔ C++ FFI design |
| [docs/architecture/](docs/architecture/) | Component diagrams |
| [docs/analysis/](docs/analysis/) | Analysis feature deep dives |
| [docs/changelog/CHANGELOG.md](docs/changelog/CHANGELOG.md) | Full changelog |
| [docs/plugins/](docs/plugins/) | Plugin development guide |
