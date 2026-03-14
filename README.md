# Fission

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

Fission is a Rust reverse-engineering workspace focused on static analysis, decompilation, and a desktop analysis workflow. The project uses Ghidra as a native backend where it is strong, but increasingly moves high-level reconstruction, normalization, and pseudocode generation into Rust-owned infrastructure.

Today the repository contains three major layers:

- a native Ghidra-backed decompiler bridge in [`/Users/sjkim1127/Fission/ghidra_decompiler`](/Users/sjkim1127/Fission/ghidra_decompiler) and [`/Users/sjkim1127/Fission/crates/fission-ffi`](/Users/sjkim1127/Fission/crates/fission-ffi)
- a split Rust analysis stack in [`/Users/sjkim1127/Fission/crates`](/Users/sjkim1127/Fission/crates)
- a Tauri 2 + React desktop frontend in [`/Users/sjkim1127/Fission/crates/fission-tauri`](/Users/sjkim1127/Fission/crates/fission-tauri)

The project currently ships:

- a CLI entrypoint: [`/Users/sjkim1127/Fission/crates/fission-cli`](/Users/sjkim1127/Fission/crates/fission-cli)
- a static analysis/decompilation backend: [`/Users/sjkim1127/Fission/crates/fission-static`](/Users/sjkim1127/Fission/crates/fission-static)
- a dynamic/runtime/debug backend: [`/Users/sjkim1127/Fission/crates/fission-dynamic`](/Users/sjkim1127/Fission/crates/fission-dynamic)
- an in-progress Fission-owned preview decompiler core under [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir)
- embedded Windows signature/type data in [`/Users/sjkim1127/Fission/crates/fission-signatures`](/Users/sjkim1127/Fission/crates/fission-signatures)

## What Fission Is Trying To Become

The long-term direction is not "a thin UI around Ghidra."

The current architecture is converging on:

- Ghidra/native C++ as the lifting, CFG, and hard-failure containment backend
- Fission-owned Rust IR as the place where high-level control flow, data abstraction, type surfacing, and pseudocode cleanup happen
- CLI/Tauri as product surfaces over the same analysis core

In practical terms:

- the `legacy` engine is still the stable default path
- the `mlil-preview` engine is the forward architecture path
- the `nir` code under [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir) is not a string post-processor; it is the start of a separate decompiler core

## Current Snapshot

As of March 14, 2026, the repository has two real decompilation paths:

### 1. `legacy`

This is the mature path:

- native Ghidra decompilation
- Fission post-processing and cleanup
- broadest type surface coverage today
- current regression guard and stable default

### 2. `mlil-preview`

This is the Fission-owned experimental path:

- native Ghidra p-code extraction
- Rust-side NIR/HIR reconstruction
- Rust-side CFG structuring
- Rust-side normalization and idiom recognition
- Rust-side type hint surfacing
- Rust-side pseudocode printer

This path now supports:

- PE x64 direct preview
- bootstrap-level PE x86 direct preview on selected seed functions
- stack-slot recovery
- straight-line lowering
- multi-block `if`, `if/else`, `while`, and `do-while`
- short-circuit folding for canonical `&&` / `||`
- cast canonicalization
- `PIECE` / `SUBPIECE` recombination
- slot/table surfacing
- slot-family recovery groundwork
- preview-only pseudo intrinsics such as `WRITE_BITS`, `FLUSH_BITS`, and `EMIT_CODE`

It still remains experimental. The important difference is that it is now a real product path exposed in both CLI and desktop UI, not an isolated prototype.

## Verified Recent Milestones

The repository has moved through several recent internal milestones. The detailed historical record lives in [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md). The short version is:

- v14 restored legacy-path benchmark stability and removed the last known benchmark `type` failures in that round
- v15 widened preview adoption and kept preview fallback/goto/temp-surface metrics at zero on the covered set
- v16 improved preview type surfacing and made [`/Users/sjkim1127/Fission/samples/windows/x64/putty.exe`](/Users/sjkim1127/Fission/samples/windows/x64/putty.exe) `0x140006260` preview output directly surface:
  - `LPRECT param_2`
  - `RECT local_3c`
  - `*param_2 = local_3c;`
- v24 recovered direct preview on representative x64 targets again and bootstrapped direct preview on at least one x86 seed
- v25 refactored the `nir` implementation into a real module tree for maintainability

Important current nuance:

- preview coverage is now high enough to be a meaningful engineering target
- preview output quality is no longer blocked only by control-flow structuring
- the next major frontier is data abstraction and large-function body readability

## Current Practical Status

If you are trying to use Fission today, the most accurate status is:

- the CLI and analysis crates are the most mature part of the repository
- the Tauri frontend is real, buildable, and connected to the same engine stack
- `legacy` is still the stable default for serious use
- `mlil-preview` is the architecture path under active investment
- `nir` is already large enough that it should be thought of as a decompiler subsystem, not a convenience helper

Current `mlil-preview` strengths:

- structured multi-block output on many real functions
- aggressive removal of surface temporaries
- reduced goto usage on covered functions
- improving control over printer output because the Rust path owns the final render

Current `mlil-preview` weaknesses:

- broader real-world type surfacing still trails `legacy`
- large-function direct-preview coverage is not complete
- data abstraction for memory slots, array/table access, and state-machine style code is still in progress
- output can still be mechanically correct but visually lower-level than desired on large functions

## Workspace Layout

Workspace members are declared in [`/Users/sjkim1127/Fission/Cargo.toml`](/Users/sjkim1127/Fission/Cargo.toml). The important crates are:

- [`/Users/sjkim1127/Fission/crates/fission-core`](/Users/sjkim1127/Fission/crates/fission-core)
  - shared core types and utilities
- [`/Users/sjkim1127/Fission/crates/fission-loader`](/Users/sjkim1127/Fission/crates/fission-loader)
  - PE/ELF/Mach-O loading
- [`/Users/sjkim1127/Fission/crates/fission-disasm`](/Users/sjkim1127/Fission/crates/fission-disasm)
  - disassembly support
- [`/Users/sjkim1127/Fission/crates/fission-pcode`](/Users/sjkim1127/Fission/crates/fission-pcode)
  - p-code model, transforms, NIR/HIR preview core
- [`/Users/sjkim1127/Fission/crates/fission-signatures`](/Users/sjkim1127/Fission/crates/fission-signatures)
  - WinAPI/type/signature data
- [`/Users/sjkim1127/Fission/crates/fission-static`](/Users/sjkim1127/Fission/crates/fission-static)
  - static analysis and decompilation orchestration
- [`/Users/sjkim1127/Fission/crates/fission-dynamic`](/Users/sjkim1127/Fission/crates/fission-dynamic)
  - debugger/runtime/plugin infrastructure
- [`/Users/sjkim1127/Fission/crates/fission-ai`](/Users/sjkim1127/Fission/crates/fission-ai)
  - future agent/platform abstraction
- [`/Users/sjkim1127/Fission/crates/fission-analysis`](/Users/sjkim1127/Fission/crates/fission-analysis)
  - compatibility facade over split analysis crates
- [`/Users/sjkim1127/Fission/crates/fission-ffi`](/Users/sjkim1127/Fission/crates/fission-ffi)
  - Rust/C++ bridge into the native decompiler
- [`/Users/sjkim1127/Fission/crates/fission-cli`](/Users/sjkim1127/Fission/crates/fission-cli)
  - CLI entrypoint
- [`/Users/sjkim1127/Fission/crates/fission-tauri/src-tauri`](/Users/sjkim1127/Fission/crates/fission-tauri/src-tauri)
  - Tauri backend

Important top-level directories:

- [`/Users/sjkim1127/Fission/ghidra_decompiler`](/Users/sjkim1127/Fission/ghidra_decompiler)
  - native decompiler sources and build tree
- [`/Users/sjkim1127/Fission/docs`](/Users/sjkim1127/Fission/docs)
  - architecture, build, benchmark, changelog, and analysis notes
- [`/Users/sjkim1127/Fission/scripts/test/batch_benchmark`](/Users/sjkim1127/Fission/scripts/test/batch_benchmark)
  - benchmark and comparison scripts
- [`/Users/sjkim1127/Fission/vendor`](/Users/sjkim1127/Fission/vendor)
  - reference codebases and third-party trees used for study and comparison

Practical ownership guidance:

- new static/decompile work belongs in [`/Users/sjkim1127/Fission/crates/fission-static`](/Users/sjkim1127/Fission/crates/fission-static)
- new preview IR/lowering/normalization/printer work belongs in [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir)
- new runtime/debug/plugin work belongs in [`/Users/sjkim1127/Fission/crates/fission-dynamic`](/Users/sjkim1127/Fission/crates/fission-dynamic)
- `fission-analysis` should be treated as a compatibility layer, not the default home for new features

## The `nir` Module Tree

The preview decompiler core now has a real directory layout:

- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/mod.rs`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/mod.rs)
  - public entrypoints and module wiring
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/builder`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/builder)
  - lowering from p-code into HIR/NIR building blocks
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/normalize`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/normalize)
  - arithmetic normalization, cleanup, slots/tables, bitstream helpers
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/structuring`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/structuring)
  - control-flow reconstruction
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/cfg.rs`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/cfg.rs)
  - CFG helpers and condition manipulation
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/piece.rs`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/piece.rs)
  - piece/subpiece reconstruction support
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/printer.rs`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/printer.rs)
  - preview pseudocode printer
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/types.rs`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/types.rs)
  - IR types and errors
- [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/tests`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/tests)
  - split test suite by feature area

This matters because Fission is no longer just appending small helpers around native output. The code organization now reflects actual subsystem boundaries:

- lowering
- normalization
- structuring
- printing
- testing

## Decompilation Architecture

At a high level, the decompilation stack looks like this:

1. load binary
2. initialize native backend and analysis context
3. choose engine
4. decompile or lift
5. apply engine-specific high-level reconstruction
6. render pseudocode

### `legacy` path

The `legacy` path is roughly:

1. native Ghidra decompilation in C++
2. Rust orchestration in [`/Users/sjkim1127/Fission/crates/fission-static/src/analysis/decomp`](/Users/sjkim1127/Fission/crates/fission-static/src/analysis/decomp)
3. post-processing passes for type promotion, expression cleanup, goto cleanup, etc.
4. final legacy C-like output

This path is still the strongest for broad type recovery and default stability.

### `mlil-preview` path

The preview path is roughly:

1. native p-code extraction
2. build HIR through [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/builder`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/builder)
3. normalize through [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/normalize`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/normalize)
4. structure through [`/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/structuring`](/Users/sjkim1127/Fission/crates/fission-pcode/src/nir/structuring)
5. apply preview type hints
6. render through the Rust printer

Important design rule:

- preview is not a legacy string-rewrite layer
- preview does not reuse legacy post-processing wholesale
- preview owns its own IR, normalization, and printer behavior

## Build Prerequisites

Minimum practical requirements:

- Rust 1.85+
- CMake 3.16+
- a working C++17 toolchain
- Node.js/npm for the Tauri frontend

Platform notes:

- Windows:
  - `zlib` is expected through `vcpkg`
- macOS:
  - the Tauri app needs full Xcode, not only Command Line Tools
- Linux:
  - GUI builds need the usual GTK/WebKit/Tauri dependencies

For fuller details, see [`/Users/sjkim1127/Fission/docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md).

## Build

### Native backend + CLI

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

Fast validation without launching the desktop app:

```bash
cargo check -p fission-tauri
cd crates/fission-tauri && npm run build
```

## CLI Quick Start

The one-shot CLI lives in [`/Users/sjkim1127/Fission/crates/fission-cli`](/Users/sjkim1127/Fission/crates/fission-cli).

Common commands:

```bash
# binary metadata
./target/release/fission_cli <binary> -i

# function list
./target/release/fission_cli <binary> -l

# strings
./target/release/fission_cli <binary> --strings 8

# decompile one function
./target/release/fission_cli <binary> --decomp 0x140001160

# disassemble one address or whole function
./target/release/fission_cli <binary> --disasm 0x140001160
./target/release/fission_cli <binary> --disasm-function 0x140001160

# batch decompile
./target/release/fission_cli <binary> --decomp-all --decomp-limit 20 --json
```

Useful decompilation flags:

- `--profile balanced|quality|speed`
- `--engine legacy|mlil-preview|auto`
- `--compiler-id auto|windows|gcc|clang|default`
- `--timeout-ms <ms>`
- `--ghidra-compat`
- `--benchmark`

Examples:

```bash
# stable path
./target/release/fission_cli <binary> --decomp 0x140001160 --engine legacy

# force preview
./target/release/fission_cli <binary> --decomp 0x140001160 --engine mlil-preview

# try preview first, then fall back
./target/release/fission_cli <binary> --decomp 0x140001160 --engine auto
```

## Desktop GUI

The current desktop UI is the Tauri project in [`/Users/sjkim1127/Fission/crates/fission-tauri`](/Users/sjkim1127/Fission/crates/fission-tauri).

What exists today:

- function list and filtering
- assembly and decompile views
- strings/imports/exports
- comments/bookmarks/function rename plumbing
- decompiler options dialog
- engine selector in the UI
- engine-used and fallback badges in the decompile view

Important policy:

- `legacy` is the stable default
- `mlil-preview` is the experimental Fission-owned path
- `auto` exists to try preview first and fall back safely

The older GUI guide at [`/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md`](/Users/sjkim1127/Fission/docs/gui/GUI_GUIDE.md) documents an earlier egui-based UI and is not the source of truth for the current Tauri frontend.

## Benchmarks and Comparison Workflow

Fission uses two benchmark styles:

### 1. Global regression benchmark

Driver:

- [`/Users/sjkim1127/Fission/scripts/test/batch_benchmark/grand_finale.py`](/Users/sjkim1127/Fission/scripts/test/batch_benchmark/grand_finale.py)

Purpose:

- broad Fission vs Ghidra regression tracking
- engine adoption statistics
- fallback/goto/temp-surface metrics

### 2. Function-by-function legacy vs preview comparison

Driver:

- [`/Users/sjkim1127/Fission/scripts/test/batch_benchmark/compare_legacy_preview.py`](/Users/sjkim1127/Fission/scripts/test/batch_benchmark/compare_legacy_preview.py)

Purpose:

- compare code quality on the same function
- compare speed
- inspect residue, cast chains, and diffs

This split matters:

- `grand_finale.py` tells you whether the product regressed globally
- `compare_legacy_preview.py` tells you whether a specific function became more readable

Repository benchmark docs:

- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.md)
- [`/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.json`](/Users/sjkim1127/Fission/docs/benchmark/grand_finale_summary.json)

## Representative Binaries Used During Development

The project frequently validates changes against a mixed set of synthetic and real-world binaries.

### Real-world x64

- [`/Users/sjkim1127/Fission/samples/windows/x64/putty.exe`](/Users/sjkim1127/Fission/samples/windows/x64/putty.exe)
  - WinAPI types, GUI-style code, medium/large functions
- [`/Users/sjkim1127/Fission/samples/windows/x64/everything.exe`](/Users/sjkim1127/Fission/samples/windows/x64/everything.exe)
  - large-function coverage, bitstream/state-machine style loops, table access
- [`/Users/sjkim1127/Fission/samples/windows/x64/notepad++.exe`](/Users/sjkim1127/Fission/samples/windows/x64/notepad++.exe)
  - large real-world GUI application, different style from PuTTY
- [`/Users/sjkim1127/Fission/vendor/x64dbg-development/cmake/cmkr.exe`](/Users/sjkim1127/Fission/vendor/x64dbg-development/cmake/cmkr.exe)
  - CLI/medium-function fallback and preview stability guard

### Real-world x86

- [`/Users/sjkim1127/Fission/samples/windows/x86/7zr.exe`](/Users/sjkim1127/Fission/samples/windows/x86/7zr.exe)
  - x86 bootstrap, split-register patterns, 32-bit stack/pointer assumptions

### Synthetic binaries

Located under [`/Users/sjkim1127/Fission/samples/windows/x64`](/Users/sjkim1127/Fission/samples/windows/x64), including:

- `test_control_flow_*`
- `test_arithmetic_idioms_*`
- `test_structs_classes_*`
- `test_string_memory_*`
- `test_real_world_algorithms_*`
- `test_advanced_patterns_*`

These are used to validate specific reconstruction and normalization behaviors in isolation.

## Preview Type Surfacing Policy

Preview type surfacing is intentionally conservative.

Current policy:

- prefer safe alias surfacing over guessed field names
- use known-signature and known-structure hints where confidence is high
- do not guess member names
- prefer pointer aliases like `LPRECT` for parameters
- prefer aggregate aliases like `RECT` for locals when the whole-object pattern is reliable

Representative direct-preview acceptance case:

- [`/Users/sjkim1127/Fission/samples/windows/x64/putty.exe`](/Users/sjkim1127/Fission/samples/windows/x64/putty.exe) `0x140006260`
  - `LPRECT param_2`
  - `RECT local_3c`
  - `*param_2 = local_3c;`

## Current Engineering Priorities

The current preview work is no longer blocked on basic enablement. The practical roadmap now looks like:

1. recover direct preview on more large functions
2. improve data abstraction
3. improve large-function body readability
4. widen x86 coverage beyond bootstrap level
5. keep `legacy` stable while preview matures

In concrete terms, the most important active areas are:

- large-function coverage recovery
- memory-slot and array/table surfacing
- bitstream/state-machine idiom recognition
- loop-body compaction
- broader preview type quality

## Known Limitations

The most important limitations today are:

- `legacy` is still the only engine that should be treated as the stable default
- `mlil-preview` still has incomplete large-function coverage
- x86 preview exists, but it is bootstrap-level support, not parity with x64
- preview body readability still lags the desired end state on large state-machine/table-driven code
- preview type surfacing is still narrower than legacy outside the current hint set

Also note:

- benchmark artifacts and local sample binaries are commonly used during development but are not meant to be committed as canonical repository deliverables
- some benchmark/data paths referenced during active development may be local-only or generated during internal runs

## Development Notes

The repository has been moving toward clearer ownership boundaries.

Recent structural changes worth knowing before editing:

- `nir` is now split by subsystem rather than kept in single giant files
- the benchmark workflow distinguishes global regressions from function-level quality comparisons
- current development typically proceeds by:
  - synthetic tests first
  - representative real-world function checks next
  - global benchmark closure last

If you are adding work in the preview path:

- prefer IR/HIR transforms over string rewrites
- prefer conservative correctness over aggressive prettification
- use synthetic tests for exact idiom recovery
- use `compare_legacy_preview.py` for function-level quality checks
- use `grand_finale.py` for broader regression closure

## Key Documentation

- [`/Users/sjkim1127/Fission/docs/README.md`](/Users/sjkim1127/Fission/docs/README.md)
  - documentation index
- [`/Users/sjkim1127/Fission/docs/build/BUILD.md`](/Users/sjkim1127/Fission/docs/build/BUILD.md)
  - build instructions
- [`/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md`](/Users/sjkim1127/Fission/docs/architecture/ARCHITECTURE.md)
  - architecture notes
- [`/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md`](/Users/sjkim1127/Fission/docs/changelog/CHANGELOG.md)
  - change history
- [`/Users/sjkim1127/Fission/docs/analysis/PASS_SYSTEM.md`](/Users/sjkim1127/Fission/docs/analysis/PASS_SYSTEM.md)
  - legacy post-processing system
- [`/Users/sjkim1127/Fission/docs/analysis/POSTPROCESS_MODULES.md`](/Users/sjkim1127/Fission/docs/analysis/POSTPROCESS_MODULES.md)
  - post-processing module notes
- [`/Users/sjkim1127/Fission/docs/cli/CLI_ONE_SHOT_MODE.md`](/Users/sjkim1127/Fission/docs/cli/CLI_ONE_SHOT_MODE.md)
  - CLI behavior
- [`/Users/sjkim1127/Fission/docs/ROADMAP.md`](/Users/sjkim1127/Fission/docs/ROADMAP.md)
  - roadmap

## Status

Fission is under active development.

The clearest way to summarize the current status is:

- `legacy` is stable and useful
- `mlil-preview` is real and increasingly capable
- `nir` is now an organized subsystem rather than a prototype blob
- the project direction is clear: Fission should own more of the high-level decompiler stack over time
