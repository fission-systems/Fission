# fission-sleigh Agent Guide

Generated: 2026-04-03
Scope: crates/fission-sleigh

## Overview

`fission-sleigh` owns the dependency-free Rust SLEIGH compiler/runtime front-end used by the `rust_sleigh` engine path.

Current ownership:

- language/spec path resolution (`specs/languages/<Processor>/**/*.slaspec`)
- compiler-only Sleigh front-end generation (`src/compiler/`, `generated/<Processor>/<entry-id>/`)
- compiled runtime registry and decode/lift contracts (`src/runtime/`)
- typed fail-closed runtime errors while generated p-code execution is incomplete
- p-code CFG block reconstruction consumed by NIR rendering

## Structure

```text
crates/fission-sleigh/
├── src/
│   ├── lib.rs
│   ├── compiler/
│   │   ├── mod.rs             # Generic compiler-only entrypoints and all-variant orchestration
│   │   ├── token.rs           # Handwritten line tokenizer
│   │   ├── preprocessor.rs    # @include/@define/conditional expansion
│   │   ├── ast.rs             # Constructor / macro / with-block AST
│   │   ├── ir.rs              # Compiled inventory + pattern graph + semantic IR
│   │   ├── codegen.rs         # Deterministic generated artifact renderer
│   │   └── equivalence.rs     # Compiler-only bucketed equivalence report
│   └── runtime/
│       ├── mod.rs             # Runtime registry, decode contracts, CFG helpers
│       └── spine/             # Ghidra-style common runtime owners and compiled-table execution
└── specs/
   └── languages/             # Local Sleigh spec set used for language resolution
       ├── AARCH64/
       ├── ARM/
       ├── MIPS/
       ├── PowerPC/
       ├── RISCV/
       ├── ...
       └── x86/
└── generated/
   ├── compiler_manifest.json # All checked-in .slaspec variant report
   ├── AARCH64/
   ├── ARM/
   ├── MIPS/
   ├── PowerPC/
   ├── RISCV/
   ├── ...
   └── x86/                   # Repo-tracked compiler-only output by entry variant
```

## Ownership Rules

1. Keep this crate dependency-free for Sleigh execution.
   - Do not reintroduce `sleigh-rs` or equivalent runtime dependencies.
   - Treat `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Processors/*/data/languages/`
     as the canonical spec source when refreshing checked-in SLEIGH files.
   - Keep the checked-in mirror aligned with `specs/ghidra_language_manifest.json`.
2. Keep runtime behavior deterministic and fail-closed.
   - Do not emit fake p-code for generated front-ends that are not executable.
   - Unsupported generated semantics must return typed runtime errors.
   - Compiler-generated artifacts must be byte-stable for the same checked-in spec snapshot.
   - Do not reintroduce bridge/oracle decode owners after generated runtime cutover.
3. Fix semantics at the canonical owner.
   - Sleigh-to-pcode mapping changes belong here, not CLI output layers.
   - Compiler-only front-end changes belong under `src/compiler/`, not benchmark or CLI glue.
   - Runtime execution ownership belongs under `src/runtime/spine/`.
   - Do not reintroduce `src/runtime/processors/`, `src/runtime/helpers/<arch>.rs`,
     `src/runtime/text/<arch>.rs`, or `src/runtime/providers/<arch>.rs`.
   - Processor differences must come from checked-in SLEIGH specs, generated IR, runtime
     registry metadata, and typed unsupported buckets until the common spine can execute them.
4. Use vendor implementations as references only.
   - Borrow invariants and ideas, but keep executable logic owned in this crate.

## Ghidra Runtime Mapping

Clean-room owner mapping is fixed as:

- `SleighLanguage` -> `RuntimeSleighFrontend` and runtime registry
- `SleighParserContext` -> `runtime::spine::RuntimeInstructionContext`
- `DecisionNode` -> `CompiledDecisionTree` plus `runtime::spine` traversal
- `ConstructState` -> `runtime::spine::RuntimeConstructState`
- `ParserWalker` -> `runtime::spine::RuntimeParserWalker`
- `ConstructTpl` -> compiler-produced templates
- `PcodeEmit` -> `runtime::spine::RuntimePcodeEmitter`

## Anti-Patterns

- Do not add CLI-layer hacks to compensate for lifting bugs.
- Do not add silent success paths when decode/runtime execution is unsupported.
- Do not hardcode binary-specific one-off rules without architectural guards.

## Validation Commands

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_sleigh_frontends
```

When changes affect CLI routing behavior:

```bash
cargo check -p fission-cli
```

## References

- `crates/fission-sleigh/src/runtime/mod.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs`
- `AGENTS.md` (repository root)
