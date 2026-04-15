# NIR Area Guide

Generated: 2026-03-27
Scope: `crates/fission-pcode/src/nir/`

## Overview

This tree owns Rust-side decompiler semantics after p-code lifting: builder state, normalization, structuring, rendering, and the canonical NIR telemetry contract.

## Structure

```text
nir/
├── builder/        # Preview/NIR lowering from p-code (see builder/AGENTS.md)
├── normalize/      # HIR normalization passes (see normalize/AGENTS.md)
├── structuring/    # CFG-driven reconstruction to higher-level HIR
├── tests/          # Synthetic NIR/structuring integration tests
├── mod.rs          # PreviewBuilder state + top-level pipeline
├── types.rs        # HIR/NIR types + NirBuildStats
└── printer.rs      # Final pseudocode rendering
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Telemetry fields | `types.rs` | `NirBuildStats` is canonical |
| PreviewBuilder state | `mod.rs`, `builder/mod.rs` | Keep builder state/projection aligned |
| Structuring rules | `structuring/` | Read child AGENT there first |
| Output formatting | `printer.rs` | Printer is downstream of semantics |
| Synthetic regression tests | `tests/` | Prefer adding targeted NIR tests here |
| Normalize pass layout | `normalize/AGENTS.md` | Directory map for `arith/`, `pipeline/`, etc. |

## Conventions

- Add new quality counters in `types.rs` first, then wire through builder snapshot/projection.
- Prefer typed helpers and deterministic ordering; many tests depend on stable output.
- Keep semantics in NIR/structuring layers, not static postprocess or UI surfaces.

## Anti-Patterns

- Do not add alternate telemetry payloads outside `NirBuildStats`.
- Do not fix structuring bugs only in `printer.rs`.
- Do not skip large-sample validation when changing rejection/acceptance logic.

## Validation

```bash
cargo test -p fission-pcode
cargo check -p fission-pcode
cargo build -p fission-cli --release
cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200
```
