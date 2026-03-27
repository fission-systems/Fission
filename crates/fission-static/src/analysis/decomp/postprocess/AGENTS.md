# Static Postprocess Guide

Generated: 2026-03-27
Scope: `crates/fission-static/src/analysis/decomp/postprocess/`

## Overview

This directory owns static-side postprocessing after decompilation output is produced. It should refine routed output, not repair missing upstream semantics that belong in `fission-pcode`.

## Structure

```text
postprocess/
├── passes.rs / pass.rs / registry.rs   # Pass registration and orchestration
├── goto_cleanup.rs / cleanup.rs        # Cleanup-oriented passes
├── structure.rs / loops.rs / switch_recon.rs / condition.rs
├── arithmetic.rs / stack_normalization.rs / piece_sweep.rs
├── aggregate_sweep.rs / var_sweep.rs / naming.rs / strings.rs
└── tests.rs                            # Postprocess-specific regression coverage
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Pass ordering | `passes.rs`, `registry.rs` | Central pass pipeline |
| Cleanup passes | `goto_cleanup.rs`, `cleanup.rs` | Downstream cleanup only |
| Structural postprocess | `structure.rs`, `loops.rs`, `switch_recon.rs` | Keep semantic ownership upstream when possible |
| Naming / strings | `naming.rs`, `strings.rs` | Presentation-adjacent refinement |

## Conventions

- Prefer fixing semantic gaps in `fission-pcode` first; use postprocess only for downstream cleanup/refinement.
- Keep passes composable and registry-driven.
- Add tests in `tests.rs` when pass ordering or postprocess semantics change.

## Anti-Patterns

- Do not patch core CFG/structuring failures only here.
- Do not duplicate type/telemetry semantics already defined upstream.
- Do not let postprocess become a hidden semantic repair layer for preview output.

## Validation

```bash
cargo check -p fission-static --features native_decomp
```
