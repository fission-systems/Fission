# NIR Structuring Guide

Generated: 2026-03-27
Scope: `crates/fission-pcode/src/nir/structuring/`

## Overview

This directory owns CFG-based reconstruction from flattened NIR/HIR into structured control flow. It is the main algorithmic hotspot for decompiler quality work.

## Structure

```text
structuring/
├── driver.rs        # Discovery / promotion orchestration
├── guards.rs        # Guarded-tail canonicalization + promotion logic
├── linear.rs        # Linear body lowering
├── loops.rs         # While / do-while / loop control rewrites
├── switch.rs        # Switch recovery
├── cfg_analysis.rs  # Dom/postdom/SCC facts
├── cleanup.rs       # Label/layout cleanup
└── conditionals/    # Plain if / if-else / short-circuit lowering
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Guarded-tail discovery / promotion | `guards.rs`, `driver.rs` | Biggest rejection buckets live here |
| Shape facts | `cfg_analysis.rs` | Prefer these over lexical heuristics |
| Conditional lowering | `conditionals/` | Shared follow / plain-if / short-circuit |
| Loop normalization | `loops.rs` | Break/continue rewriting and reducers |
| Layout cleanup | `cleanup.rs` | Canonical labels before discovery |

## Conventions

- Prefer common-follow / next-flow / dom/postdom invariants over lexical position.
- Add both positive and negative regressions for any acceptance change.
- If a new rejection bucket appears repeatedly, subtype it before broadening acceptance.
- Keep behavior deterministic; metrics and snapshots depend on stable output.

## Anti-Patterns

- Do not “fix” a guarded-tail failure by forcing a prettier printer shape.
- Do not relax nested/nonlocal/loop/switch cases without explicit structural proof.
- Do not broaden acceptance before measuring on 200/500-function automation samples.

## Validation

```bash
cargo test -p fission-pcode structuring_candidate_discovery_ -- --nocapture
cargo test -p fission-pcode structuring_ -- --nocapture
cargo test -p fission-pcode
cargo check -p fission-pcode
```
