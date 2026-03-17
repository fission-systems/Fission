# Fission

![Fission logo](./image/logo.png)

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

Rust reverse-engineering and decompilation workspace focused on **static analysis**, **p-code-driven reconstruction**, and a **desktop analysis workflow**, with a long-term direction toward a unified platform that brings together static, dynamic, and network analysis.

This repository is still an **early-stage** and **immature public codebase**. The direction is real and the core engine is advancing quickly, but the project is still under heavy development, parts of the repository are rough, and the public-facing docs are still being cleaned up.

Fission is converging on a simple architecture:

- **Ghidra as a lift service**
- **Rust as the decompiler brain**
- **CLI and Tauri as product surfaces over the same core**

Its long-term direction goes beyond single-function pseudocode. Fission is aimed at **project-level software restoration**: recovering structure, behavior, and intent from compiled artifacts, while growing toward a unified platform that combines static, dynamic, and network analysis and absorbs the strengths of traditionally fragmented reverse-engineering tools into one workflow. AI is intended to sit on top of that stack as a deeply tool-coupled workflow layer.

The repository currently includes:

- a native Ghidra-backed decompiler bridge in [`ghidra_decompiler`](./ghidra_decompiler) and [`crates/fission-ffi`](./crates/fission-ffi)
- a Rust analysis/decompilation stack in [`crates`](./crates)
- a Rust-owned preview decompiler core under [`crates/fission-pcode/src/nir`](./crates/fission-pcode/src/nir)
- a buildable CLI in [`crates/fission-cli`](./crates/fission-cli)
- a Tauri desktop frontend in [`crates/fission-tauri`](./crates/fission-tauri)

Current engine status:

- `legacy`: native Ghidra decompilation + Rust postprocess, still the stable path
- `mlil-preview`: Ghidra p-code -> Rust NIR/HIR -> structuring -> printer, the forward architecture path

Preview currently supports real PE x64 work, bootstrap-level PE x86 coverage on selected seeds, stack-slot recovery, multi-block control flow reconstruction, short-circuit folding, and Rust-owned pseudocode printing. It is still experimental, and the repository should still be treated as an evolving engineering codebase rather than a mature end-user product.

License: AGPL-3.0-or-later. Contributions are accepted under the Contributor License Agreement in [`CLA.md`](./CLA.md).

## Community

- Discord: [Fission community server](https://discord.gg/dgzqGwBpcE)
- LinkedIn: [Sung Joo Kim](https://www.linkedin.com/in/sung-joo-kim-718a93303/)

## Screenshots

Main desktop workspace:

![Fission main screen](./image/main_screen.jpeg)

Decompiler view:

![Fission decompile view](./image/decompile.jpeg)

## Project Vision

Fission is not trying to be a thin UI around Ghidra or just another decompiler frontend.

The longer-term goal is **project-level restoration**: not reconstructing the exact original source code bit-for-bit, but recovering a usable understanding of how a program is structured, what it does, and what kind of system it is trying to implement.

That long-term direction also means treating Fission as more than a decompiler. The project is aiming toward an integrated reverse-engineering platform that can combine **static analysis**, **dynamic analysis**, and **network-facing analysis**, while absorbing the strongest parts of what are usually separate and fragmented tools into a single workflow.

That means going beyond questions like "what does this function do?" and moving toward:

- how the application is organized as a whole
- what behaviors and workflows it exposes to users
- what domain concepts, protocols, and state machines exist inside the binary
- how runtime behavior and network behavior connect back to that static structure
- how those can be reconstructed into a meaningful project again

The current architecture is converging on:

- Ghidra/native C++ as the lifting, CFG, and hard-failure containment backend
- Fission-owned Rust IR as the place where high-level control flow, data abstraction, type surfacing, and pseudocode cleanup happen
- CLI/Tauri as product surfaces over the same analysis core
- AI as a future workflow layer that is tightly integrated with decompilation, analysis artifacts, and project-wide context rather than acting as a generic chatbot on the side

In practical terms:

- the `legacy` engine is still the stable default path
- the `mlil-preview` engine is the forward architecture path
- the `nir` code under [`crates/fission-pcode/src/nir`](./crates/fission-pcode/src/nir) is not a string post-processor; it is the start of a separate decompiler core

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

It still remains experimental. The important difference is that it is now a real product path exposed in both CLI and desktop UI, not an isolated demo path.

## Verified Recent Milestones

The repository has moved through several recent internal milestones. The detailed historical record lives in [`docs/changelog/CHANGELOG.md`](./docs/changelog/CHANGELOG.md). The short version is:

- v14 restored legacy-path benchmark stability and removed the last known benchmark `type` failures in that round
- v15 widened preview adoption and kept preview fallback/goto/temp-surface metrics at zero on the covered set
- v16 improved preview type surfacing and made [`samples/windows/x64/putty.exe`](./samples/windows/x64/putty.exe) `0x140006260` preview output directly surface:
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

Workspace members are declared in [`Cargo.toml`](./Cargo.toml). The important crates are:

- [`crates/fission-core`](./crates/fission-core)
  - shared core types and utilities
- [`crates/fission-loader`](./crates/fission-loader)
  - PE/ELF/Mach-O loading
- [`crates/fission-disasm`](./crates/fission-disasm)
  - disassembly support
- [`crates/fission-pcode`](./crates/fission-pcode)
  - p-code model, transforms, NIR/HIR preview core
- [`crates/fission-signatures`](./crates/fission-signatures)
  - WinAPI/type/signature data
- [`crates/fission-static`](./crates/fission-static)
  - static analysis and decompilation orchestration
- [`crates/fission-dynamic`](./crates/fission-dynamic)
  - debugger/runtime/plugin infrastructure
- [`crates/fission-analysis`](./crates/fission-analysis)
  - compatibility facade over split analysis crates
- [`crates/fission-ffi`](./crates/fission-ffi)
  - Rust/C++ bridge into the native decompiler
- [`crates/fission-cli`](./crates/fission-cli)
  - CLI entrypoint
- [`crates/fission-tauri/src-tauri`](./crates/fission-tauri/src-tauri)
  - Tauri backend

Important top-level directories:

- [`ghidra_decompiler`](./ghidra_decompiler)
  - native decompiler sources and build tree
- [`docs`](./docs)
  - architecture, build, benchmark, changelog, and analysis notes
- [`scripts/test/batch_benchmark`](./scripts/test/batch_benchmark)
  - benchmark and comparison scripts
- [`vendor`](./vendor)
  - reference codebases and third-party trees used for study and comparison

Practical ownership guidance:

- new static/decompile work belongs in [`crates/fission-static`](./crates/fission-static)
- new preview IR/lowering/normalization/printer work belongs in [`crates/fission-pcode/src/nir`](./crates/fission-pcode/src/nir)
- new runtime/debug/plugin work belongs in [`crates/fission-dynamic`](./crates/fission-dynamic)
- `fission-analysis` should be treated as a compatibility layer, not the default home for new features

## The `nir` Module Tree

The preview decompiler core now has a real directory layout:

- [`crates/fission-pcode/src/nir/mod.rs`](./crates/fission-pcode/src/nir/mod.rs)
  - public entrypoints and module wiring
- [`crates/fission-pcode/src/nir/builder`](./crates/fission-pcode/src/nir/builder)
  - lowering from p-code into HIR/NIR building blocks
- [`crates/fission-pcode/src/nir/normalize`](./crates/fission-pcode/src/nir/normalize)
  - arithmetic normalization, cleanup, slots/tables, bitstream helpers
- [`crates/fission-pcode/src/nir/structuring`](./crates/fission-pcode/src/nir/structuring)
  - control-flow reconstruction
- [`crates/fission-pcode/src/nir/cfg.rs`](./crates/fission-pcode/src/nir/cfg.rs)
  - CFG helpers and condition manipulation
- [`crates/fission-pcode/src/nir/piece.rs`](./crates/fission-pcode/src/nir/piece.rs)
  - piece/subpiece reconstruction support
- [`crates/fission-pcode/src/nir/printer.rs`](./crates/fission-pcode/src/nir/printer.rs)
  - preview pseudocode printer
- [`crates/fission-pcode/src/nir/types.rs`](./crates/fission-pcode/src/nir/types.rs)
  - IR types and errors
- [`crates/fission-pcode/src/nir/tests`](./crates/fission-pcode/src/nir/tests)
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
2. Rust orchestration in [`crates/fission-static/src/analysis/decomp`](./crates/fission-static/src/analysis/decomp)
3. post-processing passes for type promotion, expression cleanup, goto cleanup, etc.
4. final legacy C-like output

This path is still the strongest for broad type recovery and default stability.

### `mlil-preview` path

The preview path is roughly:

1. native p-code extraction
2. build HIR through [`crates/fission-pcode/src/nir/builder`](./crates/fission-pcode/src/nir/builder)
3. normalize through [`crates/fission-pcode/src/nir/normalize`](./crates/fission-pcode/src/nir/normalize)
4. structure through [`crates/fission-pcode/src/nir/structuring`](./crates/fission-pcode/src/nir/structuring)
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

For fuller details, see [`docs/build/BUILD.md`](./docs/build/BUILD.md).

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

The one-shot CLI lives in [`crates/fission-cli`](./crates/fission-cli).

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

The current desktop UI is the Tauri project in [`crates/fission-tauri`](./crates/fission-tauri).

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

The older GUI guide at [`docs/gui/GUI_GUIDE.md`](./docs/gui/GUI_GUIDE.md) documents an earlier egui-based UI and is not the source of truth for the current Tauri frontend.

## Benchmarks and Comparison Workflow

Fission uses two benchmark styles:

### 1. Global regression benchmark

Driver:

- [`scripts/test/batch_benchmark/grand_finale.py`](./scripts/test/batch_benchmark/grand_finale.py)

Purpose:

- broad Fission vs Ghidra regression tracking
- engine adoption statistics
- fallback/goto/temp-surface metrics

### 2. Function-by-function legacy vs preview comparison

Driver:

- [`scripts/test/batch_benchmark/compare_legacy_preview.py`](./scripts/test/batch_benchmark/compare_legacy_preview.py)

Purpose:

- compare code quality on the same function
- compare speed
- inspect residue, cast chains, and diffs

This split matters:

- `grand_finale.py` tells you whether the product regressed globally
- `compare_legacy_preview.py` tells you whether a specific function became more readable

Repository benchmark docs:

- [`docs/benchmark/grand_finale_summary.md`](./docs/benchmark/grand_finale_summary.md)
- [`docs/benchmark/grand_finale_summary.json`](./docs/benchmark/grand_finale_summary.json)

## Representative Binaries Used During Development

The project frequently validates changes against a mixed set of synthetic and real-world binaries.

### Real-world x64

- [`samples/windows/x64/putty.exe`](./samples/windows/x64/putty.exe)
  - WinAPI types, GUI-style code, medium/large functions
- [`samples/windows/x64/everything.exe`](./samples/windows/x64/everything.exe)
  - large-function coverage, bitstream/state-machine style loops, table access
- [`samples/windows/x64/notepad++.exe`](./samples/windows/x64/notepad++.exe)
  - large real-world GUI application, different style from PuTTY
- [`vendor/x64dbg-development/cmake/cmkr.exe`](./vendor/x64dbg-development/cmake/cmkr.exe)
  - CLI/medium-function fallback and preview stability guard

### Real-world x86

- [`samples/windows/x86/7zr.exe`](./samples/windows/x86/7zr.exe)
  - x86 bootstrap, split-register patterns, 32-bit stack/pointer assumptions

### Synthetic binaries

Located under [`samples/windows/x64`](./samples/windows/x64), including:

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

- [`samples/windows/x64/putty.exe`](./samples/windows/x64/putty.exe) `0x140006260`
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

- [`docs/README.md`](./docs/README.md)
  - documentation index
- [`docs/build/BUILD.md`](./docs/build/BUILD.md)
  - build instructions
- [`docs/architecture/ARCHITECTURE.md`](./docs/architecture/ARCHITECTURE.md)
  - architecture notes
- [`docs/changelog/CHANGELOG.md`](./docs/changelog/CHANGELOG.md)
  - change history
- [`docs/analysis/PASS_SYSTEM.md`](./docs/analysis/PASS_SYSTEM.md)
  - legacy post-processing system
- [`docs/analysis/POSTPROCESS_MODULES.md`](./docs/analysis/POSTPROCESS_MODULES.md)
  - post-processing module notes
- [`docs/cli/CLI_ONE_SHOT_MODE.md`](./docs/cli/CLI_ONE_SHOT_MODE.md)
  - CLI behavior
- [`docs/ROADMAP.md`](./docs/ROADMAP.md)
  - roadmap

## Status

Fission is under active development.

The clearest way to summarize the current status is:

- `legacy` is stable and useful
- `mlil-preview` is real and increasingly capable
- `nir` is now an organized subsystem rather than an experimental blob
- the project direction is clear: Fission should own more of the high-level decompiler stack over time
