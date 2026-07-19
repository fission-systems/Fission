<div align="center">

<img src="https://raw.githubusercontent.com/sjkim1127/Fission/main/image/logo-github.png" alt="Fission - reverse engineering workspace" width="760" />

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

</div>

---

# Fission

Fission is a Rust-native reverse-engineering and binary decompilation workspace. It is built around a Fission-owned intermediate-representation pipeline, with Ghidra-style Sleigh semantics feeding Rust-owned NIR, HIR, structuring, rendering, automation, and quality gates.

The project goal is not only to decode instructions. The goal is to produce decompiler output that is mechanically traceable, semantically defensible, and eventually readable enough to be useful in day-to-day reverse engineering.

This README is intentionally long. It is a practical orientation document for contributors, operators, and future agents working in the repository. The shorter source-of-truth references remain in `docs/`, `AGENTS.md`, and the crate-local `AGENTS.md` files.

## Table of Contents

- [Project Status](#project-status)
- [Quick Start](#quick-start)
- [Repository Tour](#repository-tour)
- [Architecture in One Page](#architecture-in-one-page)
- [Core Pipeline](#core-pipeline)
- [Crate Guide](#crate-guide)
- [CLI Guide](#cli-guide)
- [Resource Bundle and Git LFS](#resource-bundle-and-git-lfs)
- [Decompiler Quality Loop](#decompiler-quality-loop)
- [NIR and HIR Policy](#nir-and-hir-policy)
- [Sleigh Runtime](#sleigh-runtime)
- [Loader Policy](#loader-policy)
- [Static Facts](#static-facts)
- [Dynamic Analysis and Emulator](#dynamic-analysis-and-emulator)
- [Time-Travel Debugging (TTD)](#time-travel-debugging-ttd)
- [Symbolic Execution and Taint Engine](#symbolic-execution-and-taint-engine)
- [Concolic Path Exploration](#concolic-path-exploration)
- [Structuring Model](#structuring-model)
- [Telemetry and Reports](#telemetry-and-reports)
- [Testing Matrix](#testing-matrix)
- [CI and Release](#ci-and-release)
- [Development Workflow](#development-workflow)
- [Security and Malware Sample Policy](#security-and-malware-sample-policy)
- [Contributor Notes](#contributor-notes)
- [Troubleshooting](#troubleshooting)
- [Glossary](#glossary)
- [Roadmap](#roadmap)
- [Appendix A: Commands](#appendix-a-commands)
- [Appendix B: Review Checklists](#appendix-b-review-checklists)
- [Appendix C: File Ownership Index](#appendix-c-file-ownership-index)
- [Appendix D: Quality Investigation Playbook](#appendix-d-quality-investigation-playbook)
- [Appendix E: Documentation Index](#appendix-e-documentation-index)

## Project Status

- Fission is a research-heavy Rust workspace for binary loading, instruction lifting, decompiler IR, structuring, automation, and product surfaces.
- The canonical semantic layer is `fission-pcode`, especially the NIR/HIR and structuring modules.
- Application orchestration belongs in `fission-decompiler` and user-facing command routing belongs in `fission-cli`.
- Binary parsing and provenance belong in `fission-loader`.
- Static facts and native preparation services belong in `fission-static`.
- Sleigh decode and raw p-code lift behavior belong in `fission-sleigh`.
- Quality reports and automation lanes belong in `fission-automation`.
- Vendor trees are references only. Production paths must not depend on them at runtime or build time.
- Checked-in utility resources under `utils/` must be accessed through the existing resource-root mechanisms, not hardcoded absolute paths.
- Current day-to-day quality work prioritizes x86 and x86-64 decompilation correctness and readable pseudocode.

Fission should be treated as a system with strict ownership boundaries. If a semantic issue is discovered in the final pseudocode, the fix should land where the behavior is owned, not in the renderer or README-level presentation.

## Quick Start

### Clone

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission
```

### Install Requirements

- Rust 1.85 or newer.
- Git LFS for large utility resources.
- `cargo-nextest` for the preferred local test runner.
- A modern C toolchain if local fixtures or corpus binaries need to be rebuilt.

```bash
rustup update
cargo install cargo-nextest --locked
git lfs install
```

### Pull Required Runtime Assets

The Rust workspace can build without every large resource, but real decompilation needs Sleigh specifications and some quality lanes need signature or support data.

```bash
git lfs pull --include="utils/sleigh-specs/**"
git lfs pull --include="utils/signatures/**"
```

### Build the CLI

```bash
cargo build -p fission-cli --release
./target/release/fission_cli --help
```

### Run Common Checks

```bash
cargo nextest run -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-decompiler
cargo check -p fission-automation
```

## Repository Tour

The repository is organized as a Cargo workspace plus documentation, resource bundles, scripts, CI workflows, and reference-only vendor trees.

| Path | Purpose |
|---|---|
| `crates/` | Rust workspace crates. |
| `docs/` | Architecture, CLI, evaluation, release, versioning, onboarding, roadmap, and ADR documents. |
| `utils/` | Checked-in resource bundle material such as Sleigh specs and Ghidra data manifests. |
| `vendor/` | Reference-only third-party source trees and datasets. |
| `scripts/` | Local helper scripts for testing, corpus work, and maintenance. |
| `.github/workflows/` | CI, reusable workflow jobs, release tag workflow, fuzzing, and heavy gates. |
| `image/` | Project logo and icon assets. |
| `target/` | Local Cargo build output; not source. |

### Primary Documentation

- `AGENTS.md` defines repository-level engineering rules and ownership boundaries.
- `PROJECT.md` tracks the active normalize pass-pipeline migration: current per-stage status, the two pass-framework tracks, and recurring pitfalls found while migrating.
- `docs/PROJECT_MAP.md` is the quick navigation map.
- `docs/architecture/ARCHITECTURE.md` is the architecture source of truth.
- `docs/CLI.md` documents command-line behavior.
- `docs/EVALUATION.md` describes evaluation posture.
- `docs/QUALITY_METRICS.md` documents metric contracts.
- `docs/RELEASE.md` and `docs/VERSIONING.md` describe release discipline.
- `docs/adr/` records architectural decisions.
- `docs/onboarding/` contains practical first-task guides.
- `docs/roadmap/RUST_DECOMPILER_ROADMAP.md` tracks long-term direction.

## Architecture in One Page

The shortest useful architecture summary is: binary bytes are loaded by `fission-loader`, instruction semantics are lifted through `fission-sleigh`, canonical IR and decompiler semantics are owned by `fission-pcode`, orchestration is performed by `fission-decompiler`, and user or automation surfaces consume those results without inventing new semantic policy.

```
Binary bytes
  -> fission-loader
  -> fission-static facts and provenance
  -> fission-sleigh decode and p-code lift
  -> fission-pcode NIR
  -> fission-pcode HIR
  -> structuring, cleanup, rendering
  -> fission-decompiler result contracts
  -> CLI, TUI, GUI, automation, reports
```

- `fission-pcode` is the semantic owner.
- `fission-pcode::nir::structuring` is the structuring owner.
- `fission-decompiler` is the orchestration owner.
- `fission-static` is the facts and preparation owner.
- `fission-loader` is the binary-format owner.
- Printer surfaces are consume-only.
- Benchmark and report code must project canonical telemetry instead of inventing parallel counters.

## Core Pipeline

1. Load bytes from a supported binary format.
2. Classify format, architecture, sections, symbols, imports, exports, and executable regions.
3. Attach provenance and identity hints without changing IR semantics.
4. Prepare static facts needed by the decompiler.
5. Decode instructions through Sleigh language definitions.
6. Emit raw p-code in a form that can be compared against parity expectations.
7. Lower p-code into Fission NIR.
8. Normalize NIR while preserving semantics.
9. Recover type, calling convention, stack, pointer, and data-flow hints when evidence supports them.
10. Build HIR as a human-readable representation.
11. Apply structuring passes using CFG, dominance, post-dominance, SCC, and proof evidence.
12. Render pseudocode without inventing missing semantics.
13. Report telemetry through canonical counters.
14. Use automation lanes to compare quality over time.

## Crate Guide

| Crate | Role | Editing rule |
|---|---|---|
| `fission-script` | Script-facing helpers and experiments for automation or user scripting. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-automation` | Quality lanes, reports, summaries, and go/stop automation signals. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-core` | Shared core types, path configuration, resource roots, and utilities. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-loader` | Binary loading, sections, symbols, relocations, virtual types, and identity reports. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-pcode` | Canonical IR, NIR, HIR, structuring, CFG analysis, type hints, and printer. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-decompiler` | Decompilation orchestration, request/result contracts, Rust-Sleigh bridge, and render routing. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-signatures` | Signature datasets and lookup logic. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-static` | Static analysis facts, native preparation, discovery, xrefs, patches, and strings. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-dynamic` | Dynamic-analysis support surfaces. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-ttd` | Time-travel and trace-adjacent support. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-emulator` | Pure-Rust P-Code execution engine, OS HLE, TTD recording, and taint-aware concolic execution. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-solver` | Pure-Rust SMT/constraint engine: SymExpr AST, Solver node registry, path condition management. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-plugin` | Plugin contracts, manager, loader, and runtime hooks. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-cli` | Command-line product surface. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-sleigh` | Sleigh decode and p-code lift runtime. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-ai` | AI-facing assistance surfaces and integration points. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-tui` | Terminal UI with ratatui-based interaction. | Keep behavior inside its owner; do not leak semantic policy to callers. |
| `fission-dioxus` | Pure Rust desktop GUI surface. | Keep behavior inside its owner; do not leak semantic policy to callers. |

### fission-script

Script-facing helpers and experiments for automation or user scripting.

- Read `crates/fission-script/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-script`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-automation

Quality lanes, reports, summaries, and go/stop automation signals.

- Read `crates/fission-automation/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-automation`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-core

Shared core types, path configuration, resource roots, and utilities.

- Read `crates/fission-core/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-core`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-loader

Binary loading, sections, symbols, relocations, virtual types, and identity reports.

- Read `crates/fission-loader/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-loader`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-pcode

Canonical IR, NIR, HIR, structuring, CFG analysis, type hints, and printer.

- Read `crates/fission-pcode/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-pcode`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-decompiler

Decompilation orchestration, request/result contracts, Rust-Sleigh bridge, and render routing.

- Read `crates/fission-decompiler/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-decompiler`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-signatures

Signature datasets and lookup logic.

- Read `crates/fission-signatures/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-signatures`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-static

Static analysis facts, native preparation, discovery, xrefs, patches, and strings.

- Read `crates/fission-static/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-static`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-dynamic

Dynamic-analysis support surfaces.

- Read `crates/fission-dynamic/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-dynamic`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-ttd

Time-travel and trace-adjacent support.

- Read `crates/fission-ttd/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-ttd`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-plugin

Plugin contracts, manager, loader, and runtime hooks.

- Read `crates/fission-plugin/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-plugin`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-cli

Command-line product surface.

- Read `crates/fission-cli/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-cli`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-sleigh

Sleigh decode and p-code lift runtime.

- Read `crates/fission-sleigh/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-sleigh`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-ai

AI-facing assistance surfaces and integration points.

- Read `crates/fission-ai/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-ai`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-tui

Terminal UI with ratatui-based interaction.

- Read `crates/fission-tui/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-tui`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

### fission-dioxus

Pure Rust desktop GUI surface.

- Read `crates/fission-dioxus/Cargo.toml` before changing dependencies.
- Prefer local patterns already used inside `crates/fission-dioxus`.
- Do not create a parallel metric, loader rule, or semantic policy if another crate already owns it.
- Run the narrowest relevant test first, then broaden to crate checks.

## CLI Guide

The CLI is the fastest product surface for validating loader and decompiler behavior locally. It should expose capabilities, not own semantic fixes.

```bash
fission_cli --help
fission_cli info <binary>
fission_cli list <binary>
fission_cli decomp <binary> --addr 0x1400010a0
fission_cli decomp <binary> --all --json
```

- Use `info` for loader-level identity and provenance questions.
- Use `list` for function discovery questions.
- Use `decomp` for function-level pseudocode output.
- Use JSON output when comparing rows, caches, or automation artifacts.
- Keep command-line compatibility shims separate from semantic behavior.

## Resource Bundle and Git LFS

Fission uses checked-in resource trees and Git LFS for data that is too large or too operationally specific for normal Rust source files.

- `utils/sleigh-specs/` contains Sleigh language resources and manifests.
- `utils/ghidra-data/` contains reference data and provenance notes.
- `utils/MANIFEST.md` documents utility resources.
- Runtime code should use `PathConfig`, `PATHS`, `resource_roots`, or existing loaders.
- Production code must not hardcode `/Users/sjkim1127/Fission/utils`.
- CI should pull only the LFS resources each job needs.

```bash
git lfs pull --include="utils/sleigh-specs/**"
git lfs pull --include="utils/signatures/**"
fission_cli resources status
```

## Decompiler Quality Loop

Quality work starts from a concrete function or row, not a vague aggregate. The loop below is the standard operating model for decompiler-quality changes.

1. Anchor the exact row, binary, address, function name, behavior status, and quality scores.
2. Record stdout, stderr, line count, byte count, and static feature gaps.
3. Find the canonical owner: Sleigh, NIR, type recovery, structuring, cleanup, printer, benchmark, or automation.
4. Add focused coverage for the invariant.
5. Make the smallest invariant-based production change.
6. Run the targeted test first.
7. Run relevant crate-level tests and checks.
8. Run the focused source-semantic row with stale caches disabled when available.
9. Inspect artifacts, not only aggregate numbers.
10. Run broader smoke or automation checks after a focused improvement.
11. Report whether behavior mechanically changed and whether quality improved.

### Quality Evidence Rules

- A passing synthetic test is necessary but not sufficient for a decompiler-quality claim.
- A changed pseudocode file is not automatically a quality improvement.
- Aggregate metrics must not hide row-level regressions.
- Raw p-code parity must be checked before NIR or HIR interpretation when the lift is suspect.
- Ghidra may be used as a cleanroom reference for behavior, not as a copied implementation source.

## NIR and HIR Policy

NIR and HIR have different contracts.

| Layer | Contract | Consequence |
|---|---|---|
| NIR | Semantically identical to the source behavior. | Correctness and parity are the highest priorities. |
| HIR | Human-readable pseudocode derived from correct semantics. | Readability can be prioritized when the underlying semantics remain traceable. |

- NIR must not be prettified by losing behavior.
- HIR may remove unnecessary temporaries when that improves readability without hiding semantics.
- Printer code must not fake structure that the structuring owner did not prove.
- Type and data abstraction should be recovered at semantic layers, not by output-only substitution.

## Sleigh Runtime

The Sleigh runtime is the decode and raw p-code lift path. It should execute `.sla` ConstructTpl semantics rather than accumulating manual opcode mappings.

- Keep `.sla` execution as the success source.
- Raw p-code canaries are admission gates for lift changes.
- Validate x86 and x86-64 shared-token cases carefully.
- Preserve Ghidra-style materialization behavior where downstream parity depends on it.
- Do not grow legacy token cursor or compatibility-classifier debt without a clear retirement path.

## Loader Policy

The loader owns binary format parsing and metadata provenance. It does not own decompiler repair.

1. `detect` stage in the loader pipeline.
2. `probe/load-spec` stage in the loader pipeline.
3. `map` stage in the loader pipeline.
4. `symbols` stage in the loader pipeline.
5. `finalize` stage in the loader pipeline.

- Known unsupported formats should fail closed with typed messages.
- Raw binary loading must not silently become a fallback for unknown bytes.
- Container formats must be classified before executable loading.
- Loader provenance is a public contract consumed by CLI and GUI surfaces.
- Function lists must use loader-owned views rather than surface-specific filtering rules.

## Static Facts

Static facts help the decompiler, but they are not the final semantic owner. The fact layer should provide evidence, provenance, and analysis services.

- Xrefs and discovery facts.
- Patch and string facts.
- Native decompiler preparation.
- FactStore-style aggregation for decompilation contexts.
- Binary-derived helper services.

## Dynamic Analysis and Emulator

Fission includes a pure-Rust dynamic analysis engine (`fission-emulator`) capable of executing arbitrary x86/x86-64 P-Code sequences produced by Sleigh, without requiring any external runtime like QEMU or Unicorn.

The emulator is a core pillar for future dynamic reasoning capabilities:
- **Tracing and Differential Analysis** — Execute binaries with a full instruction trace and diff the output against expected behavior.
- **Time-Travel Debugging (TTD)** — Record/replay execution with memory and register delta snapshots.
- **Concolic Execution** — Use TTD snapshots to fork execution at unexplored conditional branches.
- **Symbolic Taint Tracking** — Propagate symbolic expressions through P-Code operations and track tainted data flows from inputs (e.g. `stdin`) through memory and registers.

### Architecture

```
fission-emulator
  ├── core.rs          — Emulator state, main run loop, register I/O, TTD hooks
  ├── pcode/
  │   ├── eval.rs      — P-Code opcode evaluator (taint-aware)
  │   └── state.rs     — MachineState: address spaces + shadow memory
  ├── arch/            — Architecture descriptors, calling conventions
  ├── os/
  │   ├── linux/       — Linux syscall HLE (read, write, mmap, brk, exit, ...)
  │   ├── windows/     — Windows HLE stubs
  │   └── bare_metal/  — No-OS / embedded HLE
  ├── sym/
  │   └── mod.rs       — SymbolicExecutor: TTD-backed concolic path exploration
  ├── trace.rs         — TraceLog: per-instruction audit trail
  ├── snapshot.rs      — Lightweight snapshot helpers
  └── loader.rs        — Binary loading bridge for the emulator
```

### CLI Integration

```bash
# Emulate a binary (default: auto-detect OS, no limits)
fission_cli run <binary>

# Limit to N instructions
fission_cli run <binary> --max-inst 10000

# Provide stdin mock data
fission_cli run <binary> --stdin "hello\n"

# Emit full instruction trace (JSON)
fission_cli run <binary> --trace --json

# Enable TTD recording (snapshot every 1000 instructions)
fission_cli run <binary> --ttd-record 1000

# Seek TTD to step N (rewind and replay from that point)
fission_cli run <binary> --ttd-record 1000 --ttd-seek 5000

# Enable concolic path exploration
fission_cli run <binary> --ttd-record 500 --sym-explore
```

### Operating System HLE

The emulator uses High-Level Emulation (HLE) rather than emulating an actual OS kernel. Each architecture+OS pair has its own HLE handler:

| OS | HLE Handler | Key syscalls |
|---|---|---|
| Linux x86-64 | `os/linux/mod.rs` | `read`, `write`, `mmap`, `brk`, `exit_group`, `open`, `close`, `fstat`, `stat` |
| Windows x86-64 | `os/windows/hle.rs` | `VirtualAlloc`, `HeapAlloc`, `WriteFile`, `ExitProcess`, and common NT stubs |
| Bare Metal | `os/bare_metal/mod.rs` | Minimal: `semihosting` stubs only |

HLE handlers can intercept function calls by name (via PLT/IAT hooks) or by recognized calling patterns.

### Calling Convention Support

The `arch/` subsystem provides architecture-agnostic calling convention helpers:

- `Emulator::read_arg(n)` — read the nth integer argument per the current ABI
- `Emulator::write_return_val(v)` — write to the return register
- `Emulator::simulate_return()` — pop the stack or read the link register and set PC

Currently supported: **System V AMD64 ABI** (Linux x86-64), **Microsoft x64** (Windows), and a stub for **ARM AArch64**.

---

## Time-Travel Debugging (TTD)

The `fission-ttd` crate provides a deterministic execution recorder and replayer. It is the backbone for all forms of non-linear execution analysis.

### How TTD Works

1. **Recording Phase** — As the `Emulator` executes, every N instructions (configurable via `--ttd-record <N>`) it captures:
   - Complete CPU register state (`RegisterState`)
   - Memory write deltas: `(address, old_bytes, new_bytes)`
   - Shadow state (taint) deltas: `(space_id, address, old_node_id, new_node_id)`

2. **Snapshot Storage** — Snapshots are stored in a bounded ring buffer (`TTDRecorder`) with configurable capacity. Older snapshots are evicted when capacity is reached.

3. **Seeking / Rewinding** — `Emulator::ttd_seek(step)` locates the most recent snapshot at or before `step`, then restores:
   - All GP registers from `RegisterState`
   - All memory bytes from `MemoryDelta` records
   - All symbolic taint mappings from `ShadowDelta` records

4. **Replay** — After rewinding, `emulator.run()` continues forward from the restored state.

### TTD Data Structures

| Type | Crate | Purpose |
|---|---|---|
| `TTDRecorder` | `fission-ttd` | Manages the ring buffer of `ExecutionSnapshot`s |
| `ExecutionSnapshot` | `fission-ttd` | Single-point snapshot: registers + memory + shadow deltas |
| `MemoryDelta` | `fission-ttd` | Before/after memory diff for one address range |
| `ShadowDelta` | `fission-ttd` | Before/after taint AST node diff for one byte position |
| `RegisterState` | `fission-ttd` | Full x86-64 GP register snapshot |

---

## Symbolic Execution and Taint Engine

Fission's taint and symbolic engine is a pure-Rust implementation — no Z3 bindings, no C++ FFI, no external SMT dependencies. It is designed for long-term maintainability and architecture-agnostic reasoning.

### Two-Layer Architecture

```
fission-solver            — Pure-Rust SMT/Constraint engine
  ├── ast.rs              — SymExpr: the symbolic expression AST
  └── solver.rs           — Solver: node registry, path conditions, SAT stub

fission-emulator          — Concrete + Symbolic execution
  └── pcode/
      ├── state.rs        — MachineState: shadow_memory (taint map)
      └── eval.rs         — Evaluator: taint propagation per P-Code opcode
```

### `SymExpr` AST Nodes

Every symbolic value is represented as a node in the `SymExpr` tree:

| Variant | Description |
|---|---|
| `Const { val, size }` | Concrete bitvector constant |
| `Var { id, name, size }` | Named symbolic variable (e.g. `stdin_0x4000`) |
| `Add(a, b)` / `Sub(a, b)` / `Mul(a, b)` | Integer arithmetic |
| `And(a, b)` / `Or(a, b)` / `Xor(a, b)` | Bitwise operations |
| `Shl(a, b)` / `Lshr(a, b)` | Shift operations |
| `Eq(a, b)` / `Neq(a, b)` / `Ult(a, b)` / `Ule(a, b)` | Comparisons (return 1-bit) |
| `Ite { cond, t, f }` | If-then-else |
| `Extract { expr, lsb, size }` | Bit extraction |
| `Concat(a, b)` | Bitvector concatenation |

### Shadow Memory (Taint State)

The `MachineState` holds a parallel "shadow" layer alongside the concrete memory:

```rust
// shadow_memory: (space_id, byte_address) -> SymNodeId
pub shadow_memory: HashMap<(u64, u64), u32>
```

- `space_id = 2` → CPU registers (Sleigh register address space)
- `space_id = 3` → RAM (Sleigh ram address space)

When a concrete byte is written, its shadow entry is cleared. When a symbolic value is written, its shadow entry is updated with the corresponding AST node ID.

### Taint Propagation Table

| P-Code Op | Taint Behavior |
|---|---|
| `COPY` | Propagate source shadow to destination |
| `LOAD` | Propagate shadow from RAM byte to output varnode |
| `STORE` | Propagate shadow from source varnode to RAM byte |
| `INT_ADD` | If either input is tainted, build `SymExpr::Add(a, b)` and store new node |
| `INT_SUB` | Similar: build `SymExpr::Sub` |
| Other ops | Currently concrete-only (no taint propagation yet) |

### Taint Sources

The primary taint source is `stdin`. In `os/linux/mod.rs`, the `sys_read(fd=0, ...)` handler:
1. Reads bytes from `stdin_buffer` (the `--stdin` mock).
2. Writes them into RAM as concrete bytes.
3. For each byte, calls `solver.register_var("stdin_<addr>", 1)` to create a `SymExpr::Var`.
4. Tags the corresponding `shadow_memory` entries with the new node ID.

From that point forward, any P-Code operation that reads those bytes will propagate the taint forward into new `SymExpr` constraint trees.

---

## Concolic Path Exploration

Concolic (concrete + symbolic) execution combines real execution with symbolic state to explore multiple code paths automatically.

### How It Works

1. The emulator runs normally in **concrete mode**, recording TTD snapshots along the way.
2. Every `CBranch` (conditional branch) P-Code instruction emits a `SymBranch` event containing:
   - The TTD step index at the branch point
   - The current PC value
   - Whether the branch was taken or not
   - The alternate target (address or relative P-Code index)
3. After the current path terminates, the `SymbolicExecutor` pops unexplored branches from a queue.
4. It rewinds the emulator to the snapshot closest to the branch step via `ttd_seek()`.
5. It forces the PC (or P-Code index) to the **alternate** target and resumes execution.
6. This continues until the exploration queue is empty.

```
Path 1:  [A] → [B] → [D] → halt        (branch at B taken = true)
Rewind to B
Path 2:  [A] → [B] → [C] → [E] → halt  (branch at B taken = false)
```

### `SymbolicExecutor`

The `SymbolicExecutor` (`sym/mod.rs`) is the exploration driver:

```rust
pub struct SymbolicExecutor {
    pub emu: Emulator,
    pub queue: Vec<SymBranch>,  // unexplored branch events
}
```

It calls `emu.run()` in a loop, drains `emu.sym_events` into the queue, and rewinds to the next unexplored branch. Each new execution path may reveal additional branches, which are added to the queue.

### Future: Full Symbolic Mode

The current implementation is **concolic scaffolding**: it explores paths by forcing PC values, but does not yet invert branch conditions symbolically via the `Solver`. The planned evolution:

1. When a `CBranch` is encountered and a taint variable is used in the branch condition, add the negated constraint `!condition` to `solver.assertions`.
2. Call `solver.check_sat()` to verify the alternate path is feasible.
3. Use `solver.get_value(var_id)` to obtain a concrete input that triggers the alternate path.
4. Replay with that concrete input instead of forcing PC.

This requires implementing the DPLL/CDCL bit-blasting core inside `fission-solver`, which is the next development milestone.

## Structuring Model

The active structuring path is graph-oriented and proof-driven. It should use deterministic collapse rules and explicit fallback when legality is incomplete.

- `StructureGraph` owns the collapsed overlay.
- `CollapseDriver` applies deterministic collapse rules.
- `RegionProof` records replacement and readiness evidence.
- Collapse only proof-complete and emit-ready regions.
- Fallback output should be explicit rather than disguised as clean structure.
- Printer and postprocess code must not reconstruct structure after the fact.

### Pass Pipeline Rules

There are two independent pass-orchestration tracks; neither subsumes the
other, because they operate on different IR shapes. Current migration
status and the full per-stage backlog live in `PROJECT.md`.

- **Structuring (pre-structuring, block-CFG level):** new transformations
  should be expressed as `NirPass` implementations
  (`crates/fission-pcode/src/midend/pass/`), operating on `NirFunc` (wraps
  `PreviewBuilder`'s block/CFG state) through `PassCtx` instead of capturing
  builder internals. Every pass declares an `InvariantBasis` (dominator
  tree, postdominator tree, SCC, loop body, edge classification) so review
  can reject address/function-specific overfitting. `PassOutcome::changed`
  must be accurate. Binary-specific and address-specific guards are
  forbidden in pass bodies.
- **Normalize (post-structuring, `HirFunction`/`Vec<HirStmt>` level):** new
  transformations should be expressed as `action_pipeline::Pass`
  implementations (`fission-midend-core::action_pipeline`), registered as
  `ActionGroup` entries in `fission-midend-normalize/src/pipeline/groups.rs`.
  Prefer the existing composable primitives (`fn_pass`, `cleanup_pass`,
  `gated_followup`, `admission_gated`) over new free-function control flow
  in `pipeline/stages.rs`; this migration is in progress, is done
  stage-by-stage with a real-binary before/after parity check per slice, and
  is not something to attempt in one large change. `cleanup_pass` is
  budget-gated (mirrors the legacy `run_cleanup_block` admission check on
  `EARLY_CLEANUP_BLOCK_STMT_LIMIT`/`BLOCK_LIMIT`); `fn_pass` is not — using
  the wrong one silently changes admission behavior on large functions
  without failing any test on small ones, so always check which one the
  original call site used before registering.
- A chain whose body calls something that itself needs `diag`/`perf`
  (`apply_type_signature_fixed_point`, `run_cleanup_family_passes`) cannot
  go through `fn_pass`/`GatedFollowupPass` — neither primitive carries
  `diag`/`perf` through to a callee. Keep those as a named `stage_pass` step
  instead of dropping the forwarding silently.

### Determinism

Decompiler output must be identical across separate process runs of the
same binary (`AGENTS.md` Core Rule 4). `std::collections::HashMap`/`HashSet`
use a per-process-random `RandomState` by default; any unsorted iteration
over one that feeds a `.first()`/`.find_map()`-style pick (not just a
`.contains()`/`.get()` membership check) is a real nondeterminism bug, not
a style nit — two were found and fixed this way in
`fission-pcode::midend::structuring`. When adding a new `HashMap`/`HashSet`
in `fission-pcode::midend` or `fission-midend-structuring`, prefer the
crate-local fixed-seed alias (`rustc_hash::FxBuildHasher`, already the
default there) over `std::collections::HashMap` directly, and never iterate
either collection to pick a specific value without sorting first.

## Telemetry and Reports

Telemetry is useful only if the same counter means the same thing everywhere. Fission keeps canonical decompiler counters in `NirBuildStats` and projects them outward.

- `NirBuildStats` is the canonical telemetry owner.
- Automation reports should consume canonical counters.
- Benchmark layers should not define parallel meanings for the same behavior.
- Regression reasons should map to structuring, materialization, type, or lift families.
- Reporting changes are not semantic fixes unless the row-level oracle moves.

## Testing Matrix

| Scope | Default command | Use when |
|---|---|---|
| pcode tests | `cargo nextest run -p fission-pcode` | NIR, HIR, structuring, printer, and type-hint work. |
| pcode check | `cargo check -p fission-pcode` | Compile validation after semantic changes. |
| decompiler check | `cargo check -p fission-decompiler` | Orchestration or Rust-Sleigh glue changes. |
| automation check | `cargo check -p fission-automation` | Telemetry or reporting changes. |
| core tests | `cargo nextest run -p fission-core` | Resource path and shared core changes. |
| CLI build | `cargo build -p fission-cli --release` | Product and benchmark validation requiring the release CLI. |
| workspace check | `cargo check --workspace` | Broad compile confidence before larger handoff. |

- Use `cargo nextest run` by default for Rust tests.
- Use `cargo test` for doctests or harness-specific behavior.
- Run targeted tests before crate-wide tests.
- Do not claim success from a targeted test if crate-level regression remains.
- Call out known unrelated failures explicitly.

## CI and Release

CI source of truth lives in `.github/workflows/`. Reusable workflows keep the main pipelines smaller and make heavy checks explicit.

- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
- `.github/workflows/cd.yml`
- `.github/workflows/release-tag.yml`
- `.github/workflows/fuzz.yml`
- `.github/workflows/ci-cd-monitor.yml`
- `.github/workflows/reusable-build-cli.yml`
- `.github/workflows/reusable-cli-smoke.yml`
- `.github/workflows/reusable-corpus-validation.yml`
- `.github/workflows/reusable-coverage.yml`
- `.github/workflows/reusable-lint-format.yml`
- `.github/workflows/reusable-miri.yml`
- `.github/workflows/reusable-msrv.yml`
- `.github/workflows/reusable-nir-check.yml`
- `.github/workflows/reusable-nir-regression-gate.yml`
- `.github/workflows/reusable-run-tests.yml`
- `.github/workflows/reusable-security-check.yml`
- `.github/workflows/reusable-setup-rust.yml`
- `.github/workflows/reusable-upload-artifacts.yml`
- `.github/workflows/reusable-benchmark.yml`

Release tags should be shipped through `Release Tag (CI green)`, which tags only a commit whose push run has already passed the required CI path.

## Development Workflow

1. Read the nearest `AGENTS.md` before editing a scoped area.
2. Confirm the owner layer before changing behavior.
3. Inspect existing tests and local patterns.
4. Add focused coverage for new invariants.
5. Make scoped production changes.
6. Run targeted validation.
7. Run crate-level validation.
8. Inspect Git status and stage only intended hunks.
9. Report both mechanical change and quality impact.

### Dirty Worktree Discipline

- Assume unrelated changes belong to the user.
- Do not revert changes you did not make.
- Ignore unrelated dirty files unless they block the task.
- If a touched file has user changes, read carefully and work with them.
- Use non-interactive Git commands when possible.

## Security and Malware Sample Policy

Reverse-engineering repositories often touch untrusted binaries. Treat samples and externally sourced executables as hostile inputs.

- Do not execute unknown samples as part of normal decompiler validation.
- Prefer parsing and static analysis paths.
- Keep malware sample handling aligned with `docs/MALWARE_SAMPLE_POLICY.md` and `SECURITY.md`.
- Do not upload private or suspicious binaries to third-party services without explicit approval.
- Keep sample provenance visible in reports.

## Contributor Notes

- Prefer small invariant-based fixes over address-specific special cases.
- Use CFG, dominance, post-dominance, SCC, dataflow, and fixed-point reasoning where appropriate.
- Do not add runtime dependencies on `vendor/`.
- Do not add C++ bindings to shortcut reference behavior.
- Do not bypass existing resource configuration helpers.
- Document new public contracts near the code and in `docs/` when they affect users.

## Troubleshooting

| Symptom | First check |
|---|---|
| Missing Sleigh specs | Run `git lfs pull --include="utils/sleigh-specs/**"` and check resource status. |
| CLI cannot find resources | Check `FISSION_RESOURCE_ROOT`, `--resource-root`, and `PathConfig::detect` behavior. |
| Raw p-code mismatch | Start in `fission-sleigh` before interpreting NIR output. |
| NIR is wrong but p-code is right | Investigate NIR materialization, normalization, or type hint application. |
| HIR is unreadable but NIR is right | Investigate structuring, cleanup, or printer consume behavior. |
| Report counters disagree | Trace the counter back to `NirBuildStats`. |
| Loader identifies unsupported container | Extract an executable child explicitly instead of raw-loading the container. |
| A test passes locally but CI fails | Check LFS pulls, OS-specific paths, feature flags, and reusable workflow inputs. |

## Glossary

| Term | Meaning |
|---|---|
| CFG | Control-flow graph. |
| HIR | High-level intermediate representation for readable pseudocode. |
| NIR | Normalized intermediate representation with strict semantic requirements. |
| P-code | Ghidra-style low-level instruction semantics representation. |
| Sleigh | Language specification system used for instruction decode and semantics. |
| Dominance | Graph relation used to reason about control-flow ownership. |
| Post-dominance | Graph relation used to reason about exits and structured regions. |
| SCC | Strongly connected component, often used for loop analysis. |
| RegionProof | Evidence that a region can be safely promoted during structuring. |
| NirBuildStats | Canonical NIR telemetry contract. |
| FactStore | Aggregated facts and provenance consumed by decompilation contexts. |
| FID | Function identification through signatures. |
| LFS | Git Large File Storage. |
| Taint | A label on data indicating it originated from a symbolic (untrusted) source. |
| Shadow Memory | Parallel memory map tracking symbolic AST node IDs alongside concrete bytes. |
| Concolic | Execution that combines concrete runs with symbolic state to explore multiple paths. |
| HLE | High-Level Emulation: OS syscall interception without full kernel emulation. |
| TTD | Time-Travel Debugging: record/replay execution via memory and register snapshots. |
| SymExpr | Symbolic expression AST node in `fission-solver`. |
| SAT | Boolean satisfiability problem. Used to check if a path constraint can be fulfilled. |

## Roadmap

- Improve x86 and x86-64 pseudocode quality on small sample binaries first.
- Continue strengthening control-flow recovery for if, else, switch, loop, break, and continue structures.
- Improve pointer, array, struct, and field-access expression recovery.
- Improve calling convention, parameter, local-variable, return-value, accumulator, and induction-variable cleanup.
- Maintain raw p-code parity gates for Sleigh changes.
- Improve FID and name recovery relative to signature ecosystems.
- Expand architecture and file-format breadth after x86/x86-64 quality is strong enough.
- Expand the pure-Rust symbolic execution engine: implement DPLL/CDCL bit-blasting in `fission-solver`.
- Add more P-Code taint propagation opcodes in `fission-emulator`.
- Connect taint-tracking results to decompiler type recovery.
- Continue the normalize `action_pipeline`/`ActionGroup` migration stage by stage (see `PROJECT.md` for current status and backlog); each stage's imperative `run_stage_*` free function is replaced with declarative passes, validated with a real-binary before/after check.

## Appendix A: Commands

### Build

```bash
cargo build -p fission-cli --release
cargo check --workspace
```

### Test

```bash
cargo nextest run -p fission-pcode
cargo nextest run -p fission-core
```

### Format and lint

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

### CLI smoke

```bash
./target/release/fission_cli --help
./target/release/fission_cli info <binary>
./target/release/fission_cli list <binary>
```

### Resources

```bash
git lfs pull --include="utils/sleigh-specs/**"
git lfs pull --include="utils/signatures/**"
fission_cli resources status
```

## Appendix B: Review Checklists

### Semantic change

- [ ] Owner layer is correct.
- [ ] Targeted test captures the invariant.
- [ ] No address-specific shortcut.
- [ ] No printer-only semantic patch.
- [ ] NIR correctness is preserved.
- [ ] HIR readability impact is inspected.
- [ ] Telemetry still maps to canonical counters.

### Loader change

- [ ] Format detection fails closed.
- [ ] Bounds checks are explicit.
- [ ] Provenance is preserved.
- [ ] Function views use loader rules.
- [ ] No raw fallback for unknown bytes.
- [ ] Container inputs are handled as containers.

### Sleigh change

- [ ] Raw p-code parity is checked.
- [ ] ConstructTpl execution remains the success source.
- [ ] No manual opcode mapping was introduced.
- [ ] Shared-token cursor behavior is covered.
- [ ] Canonical gates are rerun.

### Automation change

- [ ] Counters come from canonical telemetry.
- [ ] Reports remain deterministic.
- [ ] JSON contracts are stable or versioned.
- [ ] Go/stop criteria are documented.
- [ ] Artifacts remain under expected output roots.

### Documentation change

- [ ] Links resolve locally.
- [ ] Claims match current repo state.
- [ ] Commands are copy-pastable.
- [ ] Ownership boundaries are not blurred.
- [ ] Large assets are referenced through repo paths.

## Appendix C: File Ownership Index

| Path | Owner meaning |
|---|---|
| `AGENTS.md` | Repository-level contributor instructions. |
| `Cargo.toml` | Workspace members and profile settings. |
| `crates/fission-pcode/src/nir/` | Core NIR/HIR implementation. |
| `crates/fission-pcode/src/nir/structuring/` | Structuring algorithms and region collapse. |
| `crates/fission-pcode/src/nir/types/` | NIR/HIR type contracts and build stats. |
| `crates/fission-decompiler/` | Decompiler orchestration and Rust-Sleigh bridge. |
| `crates/fission-sleigh/` | Sleigh runtime. |
| `crates/fission-static/src/analysis/` | Static facts and analysis services. |
| `crates/fission-loader/src/loader/` | Binary loader implementations. |
| `crates/fission-automation/src/report/` | Automation report implementation. |
| `crates/fission-cli/src/cli/` | CLI command ownership. |
| `docs/architecture/ARCHITECTURE.md` | Architecture source of truth. |
| `docs/adr/` | Architectural decision records. |
| `utils/` | Checked-in resource bundle material. |
| `vendor/` | Reference-only third-party material. |

## Appendix D: Quality Investigation Playbook

### Raw p-code suspect

1. Compare raw p-code against a known-good reference.
2. Check Sleigh token cursor placement.
3. Check dynamic memory output materialization.
4. Add a row-level canary before broad benchmark runs.

### NIR suspect

1. Inspect p-code to NIR lowering.
2. Check temporary, register, stack, and memory varnode handling.
3. Inspect normalization rules and wave stats.
4. Validate semantic preservation before HIR cleanup.

### Type recovery suspect

1. Check calling convention source.
2. Check stack slot and parameter hints.
3. Check import and function hints.
4. Avoid output-only substitution.

### Structuring suspect

1. Inspect CFG shape.
2. Inspect dominance and post-dominance.
3. Inspect SCC and loop facts.
4. Check `RegionProof` and collapse readiness.
5. Prefer explicit fallback over invalid structure.

### Printer suspect

1. Confirm HIR is already correct.
2. Keep printer consume-only.
3. Avoid reconstructing missing control flow.
4. Add snapshot coverage if formatting changes.

## Appendix E: Documentation Index

- `docs/CLI.md`
- `docs/EVALUATION.md`
- `docs/MALWARE_SAMPLE_POLICY.md`
- `docs/PROJECT_MAP.md`
- `docs/QUALITY_METRICS.md`
- `docs/RELEASE.md`
- `docs/VERSIONING.md`
- `docs/adr/README.md`
- `docs/architecture/ARCHITECTURE.md`
- `docs/architecture/DECOMPILER_ACTIONS.md`
- `docs/architecture/DIAGRAMS.md`
- `docs/architecture/GHIDRA_PARITY_GAP_AUDIT.md`
- `docs/architecture/XREF_INDEX.md`
- `docs/contributing/LABELS.md`
- `docs/onboarding/ADDING_A_LOADER_TEST.md`
- `docs/onboarding/DEBUGGING_A_DECOMP_FAILURE.md`
- `docs/onboarding/FIRST_30_MINUTES.md`
- `docs/roadmap/RUST_DECOMPILER_ROADMAP.md`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `THIRD_PARTY.md`
- `LICENSE`

## Field Guide: Practical Rules by Area

### 01. P-code parity

Verify instruction semantics before diagnosing decompiler output.

- Owner check: identify the crate that owns p-code parity before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 02. NIR materialization

Preserve source behavior even when output is temporarily verbose.

- Owner check: identify the crate that owns nir materialization before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 03. HIR cleanup

Improve readability only after the semantic basis is correct.

- Owner check: identify the crate that owns hir cleanup before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 04. Type hints

Promote evidence-backed types and keep provenance inspectable.

- Owner check: identify the crate that owns type hints before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 05. Stack recovery

Model stack slots consistently across calls, locals, and spills.

- Owner check: identify the crate that owns stack recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 06. Pointer recovery

Prefer data-flow-backed pointer reasoning over text substitution.

- Owner check: identify the crate that owns pointer recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 07. Array recovery

Recover indexed forms when stride and base evidence are present.

- Owner check: identify the crate that owns array recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 08. Struct recovery

Recover field access only when layout evidence supports it.

- Owner check: identify the crate that owns struct recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 09. Calling convention

Derive parameters and returns from ABI facts and observed uses.

- Owner check: identify the crate that owns calling convention before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 10. Loop structuring

Use SCC and dominance facts before emitting structured loops.

- Owner check: identify the crate that owns loop structuring before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 11. Switch recovery

Use jump-table evidence and bounds before emitting switch syntax.

- Owner check: identify the crate that owns switch recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 12. Goto fallback

Use explicit fallback when legal structure is not proven.

- Owner check: identify the crate that owns goto fallback before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 13. Printer formatting

Render the model; do not create semantic facts.

- Owner check: identify the crate that owns printer formatting before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 14. Loader identity

Attach evidence without changing parse semantics.

- Owner check: identify the crate that owns loader identity before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 15. Resource lookup

Route through resource roots and path config.

- Owner check: identify the crate that owns resource lookup before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 16. Automation reports

Project canonical metrics and keep outputs deterministic.

- Owner check: identify the crate that owns automation reports before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 17. CI gates

Prefer focused reusable jobs with explicit inputs.

- Owner check: identify the crate that owns ci gates before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 18. Release tags

Tag only after successful push CI for the exact commit.

- Owner check: identify the crate that owns release tags before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 19. Vendor reference

Consult for invariants without copying or depending on code.

- Owner check: identify the crate that owns vendor reference before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 20. Documentation

Keep claims grounded in current source and docs.

- Owner check: identify the crate that owns documentation before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

## Contributor Playbooks

This section expands the repository rules into concrete scenarios. It is intentionally operational: each playbook describes where to start, what to avoid, and what evidence should exist before claiming the work is done.

### 01. Fixing a raw p-code mismatch

Start in `fission-sleigh`; compare emitted p-code before touching NIR or HIR.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 02. Fixing an NIR materialization bug

Start in `fission-pcode/src/nir`; preserve exact source behavior before readability work.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 03. Improving HIR readability

Start from correct NIR; prefer cleanup passes over printer substitutions.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 04. Changing structuring logic

Start in `fission-pcode/src/nir/structuring`; require CFG evidence and proof completeness.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 05. Adding a new NIR pass

Implement a pass with declared analysis dependencies and accurate changed status.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 06. Debugging an if-else recovery failure

Inspect dominance, post-dominance, and region exits before modifying emitted syntax.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 07. Debugging a loop recovery failure

Inspect SCCs, loop headers, latches, exits, and break or continue candidates.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 08. Debugging a switch recovery failure

Inspect jump table evidence, bounds, case targets, and default target handling.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 09. Cleaning unnecessary temporaries

Confirm the temporary has no semantic or ordering role before removing it.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 10. Recovering function parameters

Use ABI facts, call uses, stack/register evidence, and type context together.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 11. Recovering local variables

Tie stack slots, stores, loads, and lifetimes to stable local names.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 12. Recovering return values

Check accumulator registers, call sites, and observed return uses.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 13. Recovering pointer arithmetic

Prefer base plus offset or indexed forms only when data-flow evidence supports them.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 14. Recovering arrays

Require stable stride, base object, and index expression evidence.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 15. Recovering struct fields

Require layout evidence before rendering field access.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 16. Changing loader format detection

Fail closed for unknown or unsupported families; never hide uncertainty as raw bytes.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 17. Adding loader provenance

Attach evidence without changing parsing semantics or decompiler behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 18. Changing import handling

Keep true imports, import thunks, undefined externals, and debug-only symbols distinct.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 19. Changing export handling

Preserve symbol provenance and loader-owned function views.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 20. Changing resource lookup

Route through path config and resource roots; do not embed local absolute paths.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 21. Changing utility manifests

Keep manifests deterministic and explain what data is required at runtime.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 22. Changing signature lookup

Keep signature hits evidence-backed and separate from semantic repair.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 23. Changing automation reports

Project `NirBuildStats` and other canonical counters; do not redefine metrics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 24. Changing JSON report contracts

Version or document the contract and keep output deterministic.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 25. Changing CLI output

Keep CLI as a surface; do not fix semantics in formatting code.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 26. Changing CLI command parsing

Separate compatibility shims from command ownership and behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 27. Changing TUI behavior

Preserve backend contracts and avoid UI-specific semantic rules.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 28. Changing Dioxus GUI behavior

Consume shared contracts and avoid duplicate function filtering rules.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 29. Changing AI integration surfaces

Keep AI assistance advisory and preserve deterministic core behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 30. Changing plugin contracts

Keep contracts explicit, stable, and separated from core crate internals.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 31. Changing dynamic-analysis support

Keep dynamic evidence labeled and do not blur it with static facts.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 32. Changing time-travel support

Keep trace-derived facts explicit and reproducible.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 33. Adding a new test fixture

Document provenance, architecture, compiler, and why the fixture is useful.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 34. Adding a regression test

Name the invariant, not just the failing sample.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 35. Updating snapshots

Inspect semantic meaning before accepting changed text.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 36. Investigating benchmark movement

Compare exact rows, artifacts, scores, stdout, stderr, and feature gaps.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 37. Investigating a pass-count change

Trace whether the change is semantic, presentational, telemetry-only, or noise.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 38. Investigating a size change

Inspect line count and byte count together with readability and semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 39. Investigating a CI-only failure

Check OS, LFS resources, workflow inputs, feature flags, and rust version.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 40. Investigating a resource-missing failure

Check LFS pull scope, resource roots, and CLI resource status.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 41. Investigating a panic

Capture command, input, backtrace, crate owner, and minimal reproducer.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 42. Investigating non-determinism

Check map iteration, filesystem order, random seeds, timestamps, and local paths.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 43. Changing dependencies

Justify long-term maintenance value and avoid dependency shortcuts for core semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 44. Consulting Ghidra

Use it for invariants and expected behavior, not copied implementation.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 45. Consulting RetDec

Use it as reference material without creating runtime dependency.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 46. Touching vendor trees

Do not add production links, shell-outs, bindings, or copied shortcuts.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 47. Touching `utils/`

Use existing loaders and manifests instead of bypassing resource configuration.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 48. Writing architecture docs

State owner boundaries and avoid implying surface layers own semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 49. Writing user docs

Prefer commands and observed behavior over aspiration.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 50. Writing troubleshooting docs

Map symptom to first owner and first command.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 51. Writing release notes

Separate features, fixes, quality movement, and known limitations.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 52. Preparing a commit

Stage intended hunks only and keep unrelated dirty work untouched.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 53. Preparing a PR

Lead with behavior change, validation, and residual risk.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 54. Reviewing a PR

Prioritize bugs, regressions, missing tests, and ownership drift.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 55. Refactoring shared code

Keep behavior stable unless the refactor explicitly includes a measured semantic change.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 56. Adding an abstraction

Add it only when it removes real duplication or encodes a real invariant.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 57. Deleting legacy code

Prove the active path no longer depends on it and keep compatibility expectations visible.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 58. Changing telemetry names

Check every consumer and avoid parallel meanings.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 59. Changing public structs

Consider CLI JSON, GUI, automation, and downstream compatibility.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 60. Changing error types

Keep errors typed enough for users and automation to act on.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 61. Changing logging

Keep logs useful for debugging without making tests flaky.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 62. Changing performance-sensitive paths

Measure before and after when the change affects common loops.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 63. Changing memory-heavy paths

Check large binaries and avoid unbounded accumulation.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 64. Changing parser code

Use structured readers and bounds checks rather than ad hoc slicing.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 65. Changing graph algorithms

Prefer explicit graph facts over lexical ordering or sample-specific assumptions.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 66. Changing dataflow analysis

Document convergence, lattice meaning, and budget behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 67. Changing fixed-point loops

Make termination, changed status, and budget behavior inspectable.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 68. Changing type inference

Keep confidence and provenance visible; avoid overconfident names.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 69. Changing ABI handling

Keep architecture and calling convention boundaries explicit.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 70. Changing x86 behavior

Validate exact sample first, then the broader x86/x86-64 family.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 71. Changing non-x86 behavior

Do not regress x86/x86-64 priority while expanding breadth.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 72. Changing docs only

Do not claim semantic improvement from documentation changes.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 73. Changing logo or README assets

Keep icon and README logo responsibilities separate.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

## Review Question Bank

Use these questions during code review or before handing off a quality cycle.

- [ ] 01. Which layer owns this behavior?
- [ ] 02. Is the change semantic, presentational, telemetry-only, or documentation-only?
- [ ] 03. What exact row, address, sample, command, or artifact motivated the change?
- [ ] 04. What invariant does the new test encode?
- [ ] 05. Could this patch accidentally special-case one binary?
- [ ] 06. Could this have been fixed lower in the pipeline?
- [ ] 07. Does the printer now infer facts it does not own?
- [ ] 08. Does automation define a metric already owned by `NirBuildStats`?
- [ ] 09. Does the loader fail closed on unsupported input?
- [ ] 10. Does resource lookup go through path configuration?
- [ ] 11. Are vendor files used only as reference material?
- [ ] 12. Is any new dependency justified by a long-term bottleneck?
- [ ] 13. Is output deterministic across machines?
- [ ] 14. Are local absolute paths absent from production code?
- [ ] 15. Does the CLI remain a product surface rather than a semantic owner?
- [ ] 16. Does GUI code consume shared function views?
- [ ] 17. Are errors typed enough to debug?
- [ ] 18. Are logs useful without being test-sensitive?
- [ ] 19. Are large binaries handled without unbounded memory growth?
- [ ] 20. Does the pass pipeline converge with accurate changed flags?
- [ ] 21. Are analysis dependencies declared explicitly?
- [ ] 22. Is fallback output honest when structure is not proven?
- [ ] 23. Are type hints evidence-backed?
- [ ] 24. Are stack slots and registers handled consistently?
- [ ] 25. Is ABI behavior isolated by architecture?
- [ ] 26. Are sample fixtures documented?
- [ ] 27. Were snapshots inspected before acceptance?
- [ ] 28. Were stale caches disabled for semantic benchmark checks?
- [ ] 29. Were row-level artifacts inspected?
- [ ] 30. Did any existing pass row regress?
- [ ] 31. Was the release CLI rebuilt when benchmark validation needed it?
- [ ] 32. Does CI pull the right LFS resources?
- [ ] 33. Are docs updated when public behavior changes?
- [ ] 34. Are limitations stated plainly?

## Maintainer Handoff Template

Use this template when handing off a substantial decompiler-quality change.

- **Problem:**
- **Root cause:**
- **Owner layer:**
- **Implementation summary:**
- **Tests run:**
- **Benchmarks or row checks:**
- **Artifacts inspected:**
- **Quality result:**
- **Regressions checked:**
- **Known risks:**
- **Follow-up work:**
