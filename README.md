# Fission

![Fission logo](./image/logo.png)

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

Fission is a Rust-first reverse-engineering and decompilation workspace.

The current architectural direction is:

- `fission-sleigh` owns decode, instruction semantics, and lift contracts
- `fission-pcode` owns canonical IR, structuring, and pseudocode rendering
- `fission-decompiler-core` owns decompiler orchestration, routing, and postprocess application
- `fission-static` supplies facts, native prepare helpers, and static-analysis services
- `fission-cli` and `fission-tauri` are product surfaces over the same core
- Ghidra is used for comparison, benchmarking, and reference invariants, not as an active decompilation path inside Fission

This repository is active engineering code, not a polished end-user release. The Rust decompiler path is real and improving quickly, but contracts, docs, and output quality are still moving.

License: AGPL-3.0-or-later. Contributions are accepted under the CLA in [`CLA.md`](./CLA.md).

## Documentation Hub

Fission documentation is managed in a hybrid model:

- Repository docs (design decisions, contributor-facing contracts, release notes):
  - [`docs/DOCUMENTATION_HUB.md`](./docs/DOCUMENTATION_HUB.md)
  - [`docs/DOCUMENT_CLASSIFICATION_DRAFT.md`](./docs/DOCUMENT_CLASSIFICATION_DRAFT.md)
  - [`docs/changelog/CHANGELOG.md`](./docs/changelog/CHANGELOG.md)
  - [`docs/changelog/CHANGELOG.ko.md`](./docs/changelog/CHANGELOG.ko.md)
- GitHub Wiki (operator guides, tutorials, FAQ, troubleshooting):
  - [Wiki Home](https://github.com/sjkim1127/Fission/wiki)
  - [Wiki Git Repository](https://github.com/sjkim1127/Fission.wiki.git)

## What Fission Is Today

Fission currently exposes one primary decompilation path:

| Path | Role | Notes |
| --- | --- | --- |
| `nir` | Primary architecture path | Fission Sleigh lift -> Rust NIR/HIR -> structuring -> printer |

Current project bias:

- Rust owns the decompiler core
- preview-first routing is the default policy
- unsupported giant functions must fail closed into explicit fallback, not fabricated structure
- telemetry and quality gates are treated as first-class engineering inputs

## Repository Layout

Important workspace members:

| Crate | Responsibility |
| --- | --- |
| [`crates/fission-sleigh`](./crates/fission-sleigh) | Sleigh decoding, lift semantics, CFG skeleton |
| [`crates/fission-pcode`](./crates/fission-pcode) | Canonical IR, NIR/HIR, structuring, printer |
| [`crates/fission-static`](./crates/fission-static) | Static facts, native prepare helpers, analysis services |
| [`crates/fission-decompiler-core`](./crates/fission-decompiler-core) | Canonical decompiler orchestration and postprocess owner |
| [`crates/fission-loader`](./crates/fission-loader) | Binary loading, symbols, sections, strings |
| [`crates/fission-signatures`](./crates/fission-signatures) | Signature and type data |
| [`crates/fission-automation`](./crates/fission-automation) | `nir-check`, quality lanes, artifact reports |
| [`crates/fission-cli`](./crates/fission-cli) | CLI surface |
| [`crates/fission-tauri`](./crates/fission-tauri) | Desktop UI |

Secondary crates:

- [`crates/fission-analysis`](./crates/fission-analysis)
- [`crates/fission-disasm`](./crates/fission-disasm)
- [`crates/fission-core`](./crates/fission-core)
- [`crates/fission-dynamic`](./crates/fission-dynamic)
- [`crates/fission-decompiler-core`](./crates/fission-decompiler-core)

## Quick Start

Build the CLI:

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission

cargo build -p fission-cli --release
```

Basic CLI usage:

```bash
# Binary info
./target/release/fission_cli <binary> --info

# One-shot decompilation
./target/release/fission_cli <binary> --decomp <address>

# Interactive mode
./target/release/fission_cli <binary>
```

Run the main quality lane:

```bash
cargo run -p fission-automation -- nir-check --lane nir
```

For documentation map and migration plan, see [`docs/DOCUMENTATION_HUB.md`](./docs/DOCUMENTATION_HUB.md).

## Current Engineering Status

What is solid today:

- the Rust workspace builds
- the CLI is the most mature product surface
- the automation lane is wired into the canonical Rust telemetry
- the `nir` path is the primary implementation target
- Ghidra-backed comparison remains available for benchmarking and differential validation

What is still in motion:

- large-function readability
- data abstraction and memory surfacing
- richer type/name surfacing on the Rust path
- desktop polish and end-user workflow packaging

PE x64 currently has the strongest direct `nir` coverage. Other architectures and formats exist in the workspace, but they should be treated as development targets rather than equal-production claims.

## Architecture Summary

Fission is organized around four layers:

1. Instruction semantics and lifting
2. Canonical IR
3. Structured IR
4. Presentation

Practical ownership:

- lifting quality is an input-contract problem
- semantics-preserving normalization belongs in canonical IR
- control-flow recovery belongs in structured IR
- naming and formatting polish belongs at presentation time

If a function cannot be structured safely, Fission should end in explicit fallback or unstructured preview output rather than incorrect high-level code.

The full architectural source of truth is [`docs/architecture/ARCHITECTURE.md`](./docs/architecture/ARCHITECTURE.md).

## Benchmark And Quality Workflow

Fission treats benchmarking as part of the decompiler, not an afterthought.

Current workflow:

- crate and targeted tests for local correctness
- `nir-check` for regression and fast-lane telemetry
- 2-way benchmark runs against Ghidra for row-level quality tracking
- row-level canaries and lowest-similarity reports for release decisions

Important artifact locations:

- [`artifacts/fission-automation/`](./artifacts/fission-automation)
- [`artifacts/batch_benchmark/`](./artifacts/batch_benchmark)
- [`artifacts/batch_benchmark_scripts/full_decomp_benchmark.py`](./artifacts/batch_benchmark_scripts/full_decomp_benchmark.py)

Representative validation commands:

```bash
cargo test -p fission-pcode
cargo check -p fission-static
cargo check -p fission-cli
cargo test -p fission-automation

cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli

python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC \
  --output-dir artifacts/batch_benchmark/putty-latest \
  --limit 50
```

## Where To Start

If you are new to the repository, read these first:

1. [`docs/DOCUMENTATION_HUB.md`](./docs/DOCUMENTATION_HUB.md)
2. [`docs/DOCUMENT_CLASSIFICATION_DRAFT.md`](./docs/DOCUMENT_CLASSIFICATION_DRAFT.md)
3. [`docs/WIKI_TOC_DRAFT.md`](./docs/WIKI_TOC_DRAFT.md)
4. [`docs/changelog/CHANGELOG.md`](./docs/changelog/CHANGELOG.md)
5. [`docs/changelog/CHANGELOG.ko.md`](./docs/changelog/CHANGELOG.ko.md)

For contributor conventions, see [`AGENTS.md`](./AGENTS.md).

## Screenshots

Main desktop workspace:

![Fission main screen](./image/main_screen.jpeg)

Decompiler view:

![Fission decompile view](./image/decompile.jpeg)

## Community

- Discord: [Fission community server](https://discord.gg/dgzqGwBpcE)
- LinkedIn: [Sung Joo Kim](https://www.linkedin.com/in/sung-joo-kim-718a93303/)

## Long-Term Direction

Fission is not trying to be a thin UI over someone else's decompiler.

The long-term target is project-level software restoration:

- recover structure and behavior from compiled artifacts
- connect static analysis, dynamic analysis, and protocol-facing analysis
- accumulate facts across functions and binaries
- provide AI-assisted workflows on top of real analysis artifacts instead of detached text generation

That direction is real, but the current repository should still be judged by what it already does well today: a Rust-owned decompiler core, measurable quality lanes, and a buildable analysis product surface.
