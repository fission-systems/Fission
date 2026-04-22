# fission-sleigh Agent Guide

Generated: 2026-04-03
Scope: crates/fission-sleigh

## Overview

`fission-sleigh` owns a dependency-free Rust implementation of the Sleigh-to-pcode bridge used by the `rust_sleigh` engine path.

Current ownership:

- language/spec path resolution (`specs/languages/<arch>/*.slaspec`)
- compiler-only Sleigh front-end generation (`src/compiler/`, `generated/x86/`)
- direct instruction length decoding and control-flow lifting (no external Sleigh runtime dependency)
- producing `fission-pcode::PcodeOp` streams consumed by NIR rendering

## Structure

```text
crates/fission-sleigh/
├── src/
│   ├── lib.rs
│   ├── compiler/
│   │   ├── mod.rs             # Compiler-only entrypoints and x86-64 wave orchestration
│   │   ├── token.rs           # Handwritten line tokenizer
│   │   ├── preprocessor.rs    # @include/@define/conditional expansion
│   │   ├── ast.rs             # Constructor / macro / with-block AST
│   │   ├── ir.rs              # Compiled inventory + pattern graph + semantic IR
│   │   ├── codegen.rs         # Deterministic generated artifact renderer
│   │   └── equivalence.rs     # Compiler-only bucketed equivalence report
│   └── lifter/
│       ├── mod.rs             # Public SleighLifter API and dispatch
│       ├── common.rs          # Shared varnode/temp utilities
│       ├── aarch64/
│       │   ├── mod.rs         # AArch64 facade and exports
│       │   ├── semantic.rs    # AArch64 semantic decode/lift
│       │   └── control.rs     # AArch64 control-flow decode
│       └── x86/
│           ├── mod.rs         # x86 facade and exports
│           ├── length.rs      # x86 instruction length decode
│           └── control.rs     # x86 control-flow decode
└── specs/
   └── languages/             # Local Sleigh spec set used for language resolution
       ├── aarch64/
       ├── arm32/
       ├── mips/
       ├── powerpc/
       ├── riscv/
       └── x86/
└── generated/
   └── x86/                   # Repo-tracked compiler-only output for x86-64
```

## Ownership Rules

1. Keep this crate dependency-free for Sleigh execution.
   - Do not reintroduce `sleigh-rs` or equivalent runtime dependencies.
   - Treat `vendor/ghidra/ghidra_12.0.4_PUBLIC/Ghidra/Processors/*/data/languages/`
     as the canonical spec source when refreshing checked-in SLEIGH files.
2. Keep decode behavior deterministic.
   - Length decode and control-flow lifting must be stable for the same bytes/address.
   - Compiler-generated artifacts must be byte-stable for the same checked-in spec snapshot.
3. Fix semantics at the canonical owner.
   - Sleigh-to-pcode mapping changes belong here, not CLI output layers.
   - Compiler-only front-end changes belong under `src/compiler/`, not benchmark or CLI glue.
4. Use vendor implementations as references only.
   - Borrow invariants and ideas, but keep executable logic owned in this crate.

## Anti-Patterns

- Do not add CLI-layer hacks to compensate for lifting bugs.
- Do not add silent success paths when decode is actually truncated or invalid.
- Do not hardcode binary-specific one-off rules without architectural guards.

## Validation Commands

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_x86_frontend
```

When changes affect CLI routing behavior:

```bash
cargo check -p fission-cli
```

## References

- `crates/fission-sleigh/src/lifter/mod.rs`
- `crates/fission-sleigh/src/lifter/aarch64/mod.rs`
- `crates/fission-sleigh/src/lifter/aarch64/semantic.rs`
- `crates/fission-sleigh/src/lifter/aarch64/control.rs`
- `crates/fission-sleigh/src/lifter/x86/mod.rs`
- `crates/fission-sleigh/src/lifter/x86/length.rs`
- `crates/fission-sleigh/src/lifter/x86/control.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs`
- `AGENTS.md` (repository root)
