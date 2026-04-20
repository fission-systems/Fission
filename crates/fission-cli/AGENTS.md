# Fission CLI Guide

Generated: 2026-04-21  
Scope: `crates/fission-cli/`

## Overview

`fission-cli` is the human-facing one-shot surface plus operator inventory entrypoint. It owns command parsing, UX grouping, compatibility shims, and output routing. It does not own semantic decompiler repair.

## Current command model

Canonical subcommands:

- `info`
- `list`
- `disasm`
- `decomp`
- `strings`
- `inventory`

Legacy flat invocations are still supported as deprecated compatibility shims. They must normalize into the same internal execution path instead of creating a second behavior path.

## Ownership boundaries

| Area | Location | Notes |
|---|---|---|
| Canonical parser + legacy shim | `src/cli/args.rs` | Normalize to internal `OneShotArgs` |
| One-shot dispatch | `src/cli/oneshot/mod.rs` | Subcommand-driven dispatch only |
| Decomp execution | `src/cli/oneshot/decompile*/` | Output semantics should remain stable across parser refactors |
| Inventory/operator emitters | `src/cli/oneshot/inventory/` | Keep batch-only knobs off the primary human-facing path |
| Output helpers | `src/cli/output/` | Rendering/serialization only, not semantic repair |

## Rules

1. Keep canonical subcommands as the source of truth for human-facing CLI shape.
2. Keep legacy flat syntax as a translation layer only; never maintain a second execution implementation.
3. Move new flags to the subcommand that owns the behavior instead of growing one global option surface.
4. Keep inventory/batch/operator controls under `inventory`, not mixed into `decomp`.
5. Preserve JSON, benchmark, and inventory payload compatibility unless the change explicitly targets those schemas.

## Anti-patterns

- Do not add semantic repair logic in CLI parsing or output code.
- Do not add new inventory-only flags to `decomp`.
- Do not let legacy shims drift from canonical subcommand behavior.
- Do not rewrite output payloads as part of a surface-only refactor.

## Validation

```bash
cargo test -p fission-cli
cargo check -p fission-cli
cargo build -p fission-cli
```

Manual validation should cover at least:

- one `info` command
- one `list` command
- one `disasm` command
- one `decomp --addr` command
- one `inventory` command
- one legacy flat command that emits a deprecation warning
