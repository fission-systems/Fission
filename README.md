# Fission

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

Fission is a Rust reverse-engineering workspace built around three pieces:

- a native Ghidra decompiler bridge (`ghidra_decompiler` + `fission-ffi`)
- a Rust analysis and post-processing pipeline (`fission-analysis`)
- a desktop frontend built with Tauri 2 + React (`crates/fission-tauri`)

The repository currently ships a CLI entrypoint (`fission_cli`), a modular analysis backend, embedded Windows signature/type data, and an in-progress desktop GUI.

## Current Snapshot

As of March 11, 2026, the mainline tree has two decompilation paths:

- the mature `legacy` path: Ghidra-native decompilation plus Fission post-processing
- the experimental `mlil-preview` path: Ghidra p-code lifting plus Fission-owned NIR/HIR and a Rust printer

Recent stable quality work in the `legacy` path includes:

- WinAPI signature-driven type promotion
- expression cleanup for Ghidra-style `CONCAT`/piece residue
- guarded `if-goto` folding, jump threading, switch clustering, and loop recovery
- temporary-variable inlining
- stack-name normalization and generic piece-access normalization

Recent experimental engine work now in-tree:

- `legacy | mlil-preview | auto` engine selection in the CLI
- Tauri decompile-engine selection plus engine/fallback badges in the UI
- a PE x64 `mlil-preview` pipeline with:
  - stack-slot recovery
  - NIR/HIR lowering for straight-line code, simple multi-block `if`, `if/else`, `while`, `do-while`
  - label/goto pseudocode fallback when structure reconstruction is incomplete
  - Rust-side idiom recognition for basic `div/mod by power-of-two`

The checked-in legacy benchmark summary is preserved in [`docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md).

Headline numbers from that checked-in run:

- 3 binaries, 60 shared successfully decompiled functions
- Fission success count: 60/60
- Ghidra success count: 60/60
- total `goto` reduction vs Ghidra: 50.50%
- `for` loops: 16 vs 11
- `do-while` loops: 16 vs 22
- sampled run reported no timeout, OOM, or crash

Current practical status:

- `legacy` is still the default-quality path
- `mlil-preview` is integrated into the product path, but it is still an experimental subset engine
- preview coverage is increasing, but output quality on real multi-block functions still trails the legacy path

## Workspace Layout

The workspace members are declared in [`Cargo.toml`](/Users/sjkim1127/Fission/Cargo.toml):

- [`crates/fission-core`](/Users/sjkim1127/Fission/crates/fission-core): shared core types and utilities
- [`crates/fission-loader`](/Users/sjkim1127/Fission/crates/fission-loader): PE/ELF/Mach-O loading
- [`crates/fission-disasm`](/Users/sjkim1127/Fission/crates/fission-disasm): disassembly support
- [`crates/fission-pcode`](/Users/sjkim1127/Fission/crates/fission-pcode): p-code data structures and transforms
- [`crates/fission-signatures`](/Users/sjkim1127/Fission/crates/fission-signatures): embedded WinAPI/type/signature database
- [`crates/fission-analysis`](/Users/sjkim1127/Fission/crates/fission-analysis): decompilation, post-processing, CFG analysis, debugger/runtime support
- [`crates/fission-ffi`](/Users/sjkim1127/Fission/crates/fission-ffi): Rust/C++ bridge into the native decompiler
- [`crates/fission-cli`](/Users/sjkim1127/Fission/crates/fission-cli): CLI entrypoint
- [`crates/fission-tauri/src-tauri`](/Users/sjkim1127/Fission/crates/fission-tauri/src-tauri): Tauri backend for the desktop app

Related top-level directories:

- [`ghidra_decompiler`](/Users/sjkim1127/Fission/ghidra_decompiler): native decompiler build tree
- [`samples`](/Users/sjkim1127/Fission/samples): sample binaries used during development and testing
- [`scripts/test/batch_benchmark`](/Users/sjkim1127/Fission/scripts/test/batch_benchmark): benchmark runners, including `grand_finale.py`
- [`docs`](/Users/sjkim1127/Fission/docs): architecture, build, benchmark, and analysis notes

## Build Prerequisites

Minimum practical requirements:

- Rust 1.85+
- CMake 3.16+
- a working C++17 toolchain
- Node.js/npm for the Tauri frontend

Platform notes:

- Windows: `zlib` is expected through `vcpkg`
- macOS: the Tauri desktop app needs full Xcode, not just Command Line Tools
- Linux GUI builds need the usual GTK/WebKit/Tauri system packages

For fuller setup notes, use [`docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md).

## Build

### CLI + native decompiler

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission

cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
cd ..

cargo build --release --bin fission_cli --features native_decomp
```

### Tauri desktop app

```bash
cd crates/fission-tauri
npm install
npm run tauri dev
```

If you only want to validate the frontend/backend integration without launching the app, these are the fast checks:

```bash
cargo check -p fission-tauri
cd crates/fission-tauri && npm run build
```

## CLI Quick Start

The one-shot CLI entrypoint lives in [`crates/fission-cli`](/Users/sjkim1127/Fission/crates/fission-cli) and currently exposes:

```bash
# Binary metadata
./target/release/fission_cli <binary> -i

# Function list
./target/release/fission_cli <binary> -l

# Strings
./target/release/fission_cli <binary> --strings 8

# Decompile one function
./target/release/fission_cli <binary> --address 0x140001160

# Alias form
./target/release/fission_cli <binary> --decomp 0x140001160

# Disassemble one address or one whole function
./target/release/fission_cli <binary> --disasm 0x140001160
./target/release/fission_cli <binary> --disasm-function 0x140001160

# Batch decompile
./target/release/fission_cli <binary> --decomp-all --decomp-limit 20 --json
```

Useful decompilation flags from the current CLI:

- `--profile balanced|quality|speed`
- `--engine legacy|mlil-preview|auto`
- `--compiler-id auto|windows|gcc|clang|default`
- `--timeout-ms <ms>`
- `--ghidra-compat`
- `--benchmark`

Examples for the experimental engine path:

```bash
# Force the Fission-owned preview engine
./target/release/fission_cli <binary> --decomp 0x140001160 --engine mlil-preview

# Try preview first for low-risk PE x64 functions, then fall back automatically
./target/release/fission_cli <binary> --decomp 0x140001160 --engine auto
```

## Desktop GUI

The current desktop app is the Tauri project in [`crates/fission-tauri`](/Users/sjkim1127/Fission/crates/fission-tauri).

What is concretely present in-tree today:

- function list / filtering
- assembly and decompile tabs
- decompiler options dialog
- comments, bookmarks, and function rename plumbing
- search, strings, imports/exports, CFG and debug-related panels
- decompile engine selector (`legacy`, `mlil-preview`, `auto`)
- engine-used / fallback badges in the decompile view

What is still moving:

- some interaction paths in the decompile view are newer than the assembly view
- GUI documentation outside this README is not fully caught up yet

The legacy [`docs/gui/GUI_GUIDE.md`](/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md) explicitly documents an older egui-based UI and should not be treated as the source of truth for the current Tauri frontend.

## Benchmarks

The repository now keeps benchmark artifacts and summaries under [`docs/benchmark`](/Users/sjkim1127/Fission/docs/benchmark).

Relevant files:

- [`docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md): latest sampled Fission vs Ghidra summary
- [`docs/benchmark/grand_finale_summary.json`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.json): machine-readable summary
- [`scripts/test/batch_benchmark/grand_finale.py`](/Users/sjkim1127/Fission/scripts/test/batch_benchmark/grand_finale.py): benchmark driver

## Key Docs

- [`docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md): build instructions
- [`docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md): architecture notes
- [`docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md): project changelog
- [`docs/analysis/PASS_SYSTEM.md`](/Users/sjkim1127/Fission/docs/analysis/PASS_SYSTEM.md): post-processing pass system
- [`docs/analysis/POSTPROCESS_MODULES.md`](/Users/sjkim1127/Fission/docs/analysis/POSTPROCESS_MODULES.md): cleanup/type-propagation pass notes
- [`docs/cli/CLI_ONE_SHOT_MODE.md`](/Users/sjkim1127/Fission/docs/cli/CLI_ONE_SHOT_MODE.md): CLI behavior
- [`docs/ROADMAP.md`](/Users/sjkim1127/Fission/docs/ROADMAP.md): roadmap

## Status

Fission is under active development.

- The CLI and analysis backend are the most mature parts of the repository.
- The Tauri frontend is real, buildable, and now exposes decompile engine selection, but its interaction model is still evolving.
- The `legacy` engine is currently the only path that should be treated as the stable default for serious analysis.
- The `mlil-preview` engine is now wired through CLI and GUI, but should still be treated as an experimental architecture path rather than a drop-in replacement for the legacy decompiler.
