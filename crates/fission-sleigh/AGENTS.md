# fission-sleigh Agent Guide

Generated: 2026-04-03
Scope: crates/fission-sleigh

## Overview

`fission-sleigh` owns the dependency-free Rust SLEIGH compiler/runtime front-end used by the `rust_sleigh` engine path.

Current ownership:

- language/spec path resolution (`utils/sleigh-specs/languages/<Processor>/**/*.slaspec`)
- checked-in compiled `.sla` overlay (`utils/sleigh-specs/compiled/<arch>/<entry>.sla`) — **required** for production lift
- compiler-only Sleigh front-end generation (`src/compiler/`, `target/fission-sleigh/generated/<Processor>/<entry-id>/`)
- compiled runtime registry and decode/lift contracts (`src/runtime/`)
- typed fail-closed runtime errors while generated p-code execution is incomplete
- p-code CFG block reconstruction consumed by NIR rendering

Execution model: **SpecDerived-only ConstructTpl**. Slaspec lowering collects metadata, layout, and codegen inventory; executable subtables, decision trees, and ConstructTpl templates come exclusively from the checked-in `.sla` overlay. There is no native decode DLL path and no slaspec-derived decision-tree fallback on the success path.

## Structure

```text
crates/fission-sleigh/
├── src/
│   ├── lib.rs
│   ├── compiler/
│   │   ├── mod.rs             # Public compiler facade; required `.sla` overlay application
│   │   ├── discovery.rs       # Spec root, manifest, compiled `.sla` resolver
│   │   ├── policy.rs          # Runtime status, executable candidate, alias/canonical mapping policy
│   │   ├── token.rs           # Handwritten line tokenizer
│   │   ├── preprocessor.rs    # @include/@define/conditional expansion
│   │   ├── ast.rs             # Constructor / macro / with-block AST
│   │   ├── ir/                # Compiled inventory + pattern graph + semantic IR
│   │   │   ├── mod.rs         # Public compatibility shim
│   │   │   ├── types.rs       # Frontend/layout/constructor type definitions
│   │   │   ├── lowering.rs    # Slaspec metadata lowering + `.sla` overlay merge
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
│       ├── frontend.rs        # Frontend construction and registry selection
│       ├── decode.rs          # Instruction/window decode contract handling
│       ├── lift.rs            # Instruction-to-function lifting orchestration
│       ├── diagnostics.rs     # Runtime trace/debug env ownership
│       ├── function.rs        # Raw p-code function lifting and CFG block construction
│       └── spine/             # Ghidra-style common runtime owners and compiled-table execution
│           └── compiled_table/ # Compiled-table decode/lift split by owner
│               ├── mod.rs     # Public decode entrypoints
│               ├── context.rs # Instruction/context bits
│               ├── strategy.rs # Compiled-table decode strategy (common spine only)
│               ├── selection.rs # SLA decision traversal and terminal verification
│               ├── walker.rs  # ParserWalker-style binding
│               ├── handles.rs # Fixed/exported handle materialization
│               ├── token.rs   # SLA token field reads (canonical operand bit extraction)
│               ├── display.rs # Display rendering
│               ├── template_eval.rs # ConstructTpl execution/emitter adapter
│               └── tests.rs   # Runtime tests
└── target/fission-sleigh/generated/
   ├── compiler_manifest.json # Build-cache .slaspec variant report
   ├── AARCH64/
   ├── ARM/
   ├── MIPS/
   ├── PowerPC/
   ├── RISCV/
   ├── ...
   └── x86/                   # Build-cache compiler-only output by entry variant
```

## Ownership Rules

1. Keep this crate dependency-free for Sleigh execution.
   - Do not reintroduce `sleigh-rs`, native decode DLLs, or equivalent runtime dependencies.
   - Treat `vendor/ghidra/` as reference-only for algorithms and invariants.
   - Production resources live under `utils/sleigh-specs/` (`languages/`, `compiled/`, manifest).
   - Keep the checked-in `utils/sleigh-specs` snapshot aligned with
     `utils/sleigh-specs/ghidra_language_manifest.json`.
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
   - Processor differences must come from checked-in SLEIGH specs, `.sla` overlay IR,
     runtime registry metadata, and typed unsupported buckets.
4. Use vendor implementations as references only.
   - Borrow invariants and ideas, but keep executable logic owned in this crate.

## Internal Layering

The crate is intentionally structured for a future split into compiler/runtime/SLA
crates. Keep dependency direction clean even while everything still lives in one
crate:

- `compiler::discovery` owns paths, manifest lookup, and `utils/sleigh-specs/compiled/` resolution.
- `compiler::policy` owns runtime status, executable candidate, compatibility
  aliases, and canonical processor mapping. Do not duplicate this policy in runtime.
- `compiler::ir` owns generated artifact shapes. Preserve serde field names and
  generated JSON compatibility. Slaspec lowering fills metadata; `.sla` overlay fills executable subtables.
- `compiler::sla` owns packed `.sla` decoding and maps only into compiler IR types.
  It must not depend on runtime modules.
- `runtime::frontend` owns frontend construction and registry selection.
- `runtime::decode` owns instruction/window decode contract handling.
- `runtime::lift` owns instruction-to-function lifting orchestration.
- `runtime::diagnostics` owns runtime trace/debug env handling.
- `runtime::function` owns CFG reconstruction.
- `runtime::spine::compiled_table` is split by owner. `context`, `selection`,
  `strategy`, `walker`, `handles`, `display`, and `template_eval` should not grow
  cross-cutting orchestration logic.

Future crate split boundary:

- compiler layers must not import runtime types.
- runtime layers may consume compiler facade/IR types but must not call compiler
  orchestration or generated-artifact writers.
- SLA decoder code may know IR template types only; it must not know runtime state.

## Ghidra Runtime Mapping

Clean-room owner mapping is fixed as:

- `SleighLanguage` -> `RuntimeSleighFrontend` and runtime registry
- `SleighParserContext` -> `runtime::spine::RuntimeInstructionContext`
- `DecisionNode` -> `CompiledDecisionTree` plus `runtime::spine` traversal
- `ConstructState` -> `runtime::spine::RuntimeConstructState`
- `ParserWalker` -> `runtime::spine::RuntimeParserWalker`
- `ConstructTpl` -> `CompiledConstructTpl` / `CompiledConstructorTemplate`
- `PcodeEmit` -> `runtime::spine::RuntimePcodeEmitter`

Runtime policy is driven by checked-in `.sla` decision trees, token/context fields,
operand specs, display templates, and `ConstructTpl` execution with
`CompiledTemplateSource::SpecDerived`. `token.rs` reads SLA token field metadata;
`BoundOperand` is display/debug-only and must not become a pcode emit fallback.

## Anti-Patterns

- Do not add CLI-layer hacks to compensate for lifting bugs.
- Do not add silent success paths when decode/runtime execution is unsupported.
- Do not hardcode binary-specific one-off rules without architectural guards.
- Do not rebuild slaspec decision trees or ConstructTpl as a parallel success path.
- Do not reintroduce native decode DLL loading or `CompatibilityLowered` template sources.

## Validation Commands

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_sleigh_frontends
FISSION_GHIDRA_DIR=/nonexistent cargo nextest run -p fission-sleigh
```

When changes affect CLI routing behavior:

```bash
cargo check -p fission-cli
```

## References

- `crates/fission-sleigh/src/runtime/mod.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs`
- `AGENTS.md` (repository root)
