# Fission Agent Guide

Generated: 2026-03-27
Scope: repository root

## Overview

Fission is a Rust-first reverse-engineering/decompilation workspace. Ghidra-native lifting feeds Rust-owned NIR/HIR normalization, structuring, rendering, and automation quality lanes.

## Structure

```text
Fission/
├── crates/
│   ├── fission-pcode/        # Canonical IR, NIR/HIR, structuring, printer
│   ├── fission-static/       # Static orchestration, preview routing, fact application
│   ├── fission-automation/   # Quality lanes, deltas, go/stop signals, artifacts
│   ├── fission-loader/       # Binary parsing, symbols, sections, strings
│   ├── fission-signatures/   # FID/signature data and lookup
│   ├── fission-cli/          # CLI surface
│   └── fission-tauri/        # Desktop surface
├── ghidra_decompiler/        # Native lift/decompiler integration, built with CMake
├── vendor/                   # Ghidra, RetDec, other reference code
├── scripts/test/             # Smoke / fuzz / automation helpers
└── .github/workflows/        # CI/CD source of truth
```

## Child AGENTS

- `crates/fission-pcode/src/nir/AGENTS.md`
- `crates/fission-pcode/src/nir/structuring/AGENTS.md`
- `crates/fission-automation/src/AGENTS.md`
- `crates/fission-static/src/analysis/decomp/postprocess/AGENTS.md`

Read the nearest child file before editing those areas.

## Where To Look

| Task | Location | Notes |
|---|---|---|
| NIR structuring / canonicalization | `crates/fission-pcode/src/nir/structuring/` | Core algorithmic decompiler work lives here |
| NIR telemetry contract | `crates/fission-pcode/src/nir/types.rs` | `NirBuildStats` is canonical |
| Automation summaries / deltas | `crates/fission-automation/src/report.rs` | Must stay aligned with `NirBuildStats` |
| Static orchestration / postprocess | `crates/fission-static/src/analysis/` | Routing and downstream passes |
| Native lift boundary | `ghidra_decompiler/`, `crates/fission-ffi/` | Keep ownership separate from Rust structuring |
| Reference algorithms | `vendor/ghidra/`, `vendor/retdec-5.0/` | Use for invariants, not binary-specific heuristics |

## Core Rules

1. Fix behavior at the canonical owner, not downstream UI/surface layers.
2. Prefer algorithmic CFG / dom / postdom / SCC facts over lexical or binary-specific heuristics.
3. Use typed contracts; do not invent parallel telemetry payloads outside `NirBuildStats`.
4. Keep behavior deterministic when outputs feed snapshots, metrics, or automation comparisons.
5. Large refactors are acceptable when they reduce long-term complexity and tighten ownership.

## Anti-Patterns

- Do not patch semantic gaps only in printer/UI output.
- Do not add one-off binary-specific heuristics without invariant-based guards.
- Do not duplicate the same metric definition across pcode and automation.
- Do not treat `fission-cli` or `fission-tauri` as semantic repair layers.
- Do not claim success from one targeted test if crate-level regression remains.

## Build / Test Commands

```bash
# Native decompiler
cmake -S ghidra_decompiler -B ghidra_decompiler/build -DCMAKE_BUILD_TYPE=Release
cmake --build ghidra_decompiler/build --config Release

# CLI with native integration
cargo build -p fission-cli --features native_decomp

# Common decompiler validation
cargo test -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-automation

# Quality lane
cargo run -p fission-automation -- nir-check --lane nir
```

## Workflow Bias

- For NIR/structuring changes: targeted tests → `cargo test -p fission-pcode` → `cargo check -p fission-pcode`.
- If telemetry/reporting changes: also run `cargo check -p fission-automation`.
- Use `.github/workflows/ci.yml` and `ci-heavy.yml` as CI source of truth.

## References

- `docs/architecture/ARCHITECTURE.md`
- `docs/build/BUILD.md`
- `README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
