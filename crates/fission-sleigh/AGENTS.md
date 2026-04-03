# fission-sleigh Agent Guide

Generated: 2026-04-03
Scope: crates/fission-sleigh

## Overview

`fission-sleigh` is the Rust bridge between Sleigh language specs and Fission p-code operations.
It owns:

- Sleigh spec loading and constructor matching
- decode state handling (instruction bytes + context bits)
- conversion from `sleigh-rs` semantic statements/expressions into `fission-pcode::PcodeOp`

## Structure

```text
crates/fission-sleigh/
├── src/
│   ├── lifter/                # Sleigh loading, matching, decode/lift entrypoints
│   │   └── mod.rs
│   ├── converter/             # Statement/expression lowering to PcodeOp
│   │   ├── mod.rs
│   │   ├── assignment.rs
│   │   ├── branch.rs
│   │   ├── memory.rs
│   │   ├── unary.rs
│   │   ├── expr.rs
│   │   ├── export.rs
│   │   └── tests.rs
│   └── builder/
│       └── mod.rs             # Placeholder/basic block builder surface
└── specs/
    └── languages/             # Local Sleigh spec set used by lifter
```

## Ownership Rules

1. Fix semantics at the canonical owner.
   - If the bug is in Sleigh semantic construction/resolution, patch `vendor/sleigh-rs`.
   - If the bug is in Fission lowering, patch `crates/fission-sleigh/src/converter`.
2. Keep decode behavior deterministic for probe-based callers.
   - Any change in decoded length, matcher filters, or context seeding must preserve cursor stability.
3. Prefer explicit typed lowering over text-style heuristics.
   - Use `sleigh-rs` semantic types (`Statement`, `ExprElement`, `TableExportType`, etc.).
4. Do not silently change architecture mapping or language selection policy here.
   - Engine selection lives in CLI/static orchestration layers.
5. Preserve compatibility with Ghidra 11.4.2 spec semantics when resolving behavior mismatches.

## Anti-Patterns

- Do not patch output formatting in downstream UI to hide lift/convert bugs.
- Do not add architecture-specific hacks without semantic guards.
- Do not bypass context handling for quick fixes; seed and bit mapping must stay consistent.
- Do not weaken all errors into generic success paths; prefer targeted handling.

## Validation Commands

```bash
cargo test -p fission-sleigh
cargo check -p fission-sleigh
```

When changes affect integration with CLI decode routing:

```bash
cargo check -p fission-cli --features native_decomp
```

For the arm64 AppleSilicon parse regression specifically:

```bash
cargo test -p fission-sleigh aarch64_apple_silicon_spec_parses -- --nocapture
```

## References

- `crates/fission-sleigh/src/lifter/mod.rs`
- `crates/fission-sleigh/src/converter/mod.rs`
- `vendor/sleigh-rs/src/semantic/inner/`
- `docs/changelog/CHANGELOG.md`
