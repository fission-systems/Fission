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
│   │   ├── mod.rs             # Public compiler facade and compatibility exports
│   │   ├── discovery.rs       # Spec root, manifest, generated path, Ghidra install discovery
│   │   ├── policy.rs          # Runtime status, executable candidate, alias/canonical mapping policy
│   │   ├── token.rs           # Handwritten line tokenizer
│   │   ├── preprocessor.rs    # @include/@define/conditional expansion
│   │   ├── ast.rs             # Constructor / macro / with-block AST
│   │   ├── ir/                # Compiled inventory + pattern graph + semantic IR
│   │   │   ├── mod.rs         # Public compatibility shim
│   │   │   ├── types.rs       # Frontend/layout/constructor type definitions
│   │   │   ├── lowering.rs    # AST/preprocessed spec lowering
│   │   │   ├── template.rs    # Template compatibility lowering helpers
│   │   │   └── tests.rs       # IR tests
│   │   ├── sla/               # Compiled .sla payload decoder
│   │   │   ├── mod.rs         # Public loader API
│   │   │   ├── packed.rs      # Packed element/parser constants
│   │   │   ├── symbols.rs     # Source files, spaces, symbol tables
│   │   │   ├── templates.rs   # ConstructTpl/OpTpl decode
│   │   │   ├── display.rs     # print/opprint/display metadata
│   │   │   └── tests.rs       # SLA decoder tests
│   │   ├── codegen.rs         # Deterministic generated artifact renderer
│   │   └── equivalence.rs     # Compiler-only bucketed equivalence report
│   └── runtime/
│       ├── mod.rs             # Public runtime facade, decode contracts, shared types
│       ├── function.rs        # Raw p-code function lifting and CFG block construction
│       └── spine/             # Ghidra-style common runtime owners and compiled-table execution
│           └── compiled_table/ # Compiled-table decode/lift split by owner
│               ├── mod.rs     # Public decode entrypoints
│               ├── context.rs # Instruction/context bits
│               ├── selection.rs # Decision traversal and terminal verification
│               ├── walker.rs  # ParserWalker-style binding
│               ├── token.rs   # Legacy shared-token cursor compatibility debt
│               ├── display.rs # Display rendering
│               ├── lift.rs    # Template execution/emitter adapter
│               └── tests.rs   # Runtime tests
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

## Internal Layering

The crate is intentionally structured for a future split into compiler/runtime/SLA
crates. Keep dependency direction clean even while everything still lives in one
crate:

- `compiler::discovery` owns paths, manifest lookup, and Ghidra install discovery.
- `compiler::policy` owns runtime status, executable candidate, compatibility
  aliases, and canonical processor mapping. Do not duplicate this policy in runtime.
- `compiler::ir` owns generated artifact shapes. Preserve serde field names and
  generated JSON compatibility.
- `compiler::sla` owns packed `.sla` decoding and maps only into compiler IR types.
  It must not depend on runtime modules.
- `runtime::function` owns function lifting and CFG reconstruction.
- `runtime::spine::compiled_table` is split by owner. `context`, `selection`,
  `walker`, `display`, `template_eval`, and `legacy_token_policy` should not grow
  cross-cutting orchestration logic.

Future crate split boundary:

- compiler layers must not import runtime types.
- runtime layers may consume compiler facade/IR types but must not call compiler
  orchestration or generated-artifact writers.
- SLA decoder code may know IR template types only; it must not know runtime state.
- generated/native backend loading belongs in runtime frontend/registry ownership.

## Ghidra Runtime Mapping

Clean-room owner mapping is fixed as:

- `SleighLanguage` -> `RuntimeSleighFrontend` and runtime registry
- `SleighParserContext` -> `runtime::spine::RuntimeInstructionContext`
- `DecisionNode` -> `CompiledDecisionTree` plus `runtime::spine` traversal
- `ConstructState` -> `runtime::spine::RuntimeConstructState`
- `ParserWalker` -> `runtime::spine::RuntimeParserWalker`
- `ConstructTpl` -> `CompiledConstructTpl` / `CompiledConstructorTemplate`
- `PcodeEmit` -> `runtime::spine::RuntimePcodeEmitter`

Runtime policy must be driven by token/context fields, constructor patterns,
display templates, and compiled p-code templates. The current compiled-table
executor may still report `compatibility_lowered` template usage while the
Ghidra `ConstructTpl -> PcodeEmit` migration is incomplete. Architecture-specific
byte parsers or mnemonic-family emitters are transitional compatibility debt and
must not be moved into new `helpers`, `providers`, `quirks`, or `text` modules.
If compatibility cursor behavior must remain temporarily, keep it behind
`legacy_token_policy` and document it as debt rather than a canonical architecture
provider.

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
