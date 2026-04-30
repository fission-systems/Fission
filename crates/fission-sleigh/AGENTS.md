# fission-sleigh Agent Guide

Generated: 2026-04-03
Scope: crates/fission-sleigh

## Overview

`fission-sleigh` owns the dependency-free Rust SLEIGH compiler/runtime front-end used by the `rust_sleigh` engine path.

Current ownership:

- language/spec path resolution (`specs/languages/<Processor>/**/*.slaspec`)
- compiler-only Sleigh front-end generation (`src/compiler/`, `target/fission-sleigh/generated/<Processor>/<entry-id>/`)
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
│   │   │   ├── mod.rs         # Public compatibility shim; `types_shim` / `lowering_shim` + reexports
│   │   │   ├── types/         # Compiled IR structs/enums (`include!` chain in `types_shim`)
│   │   │   ├── lowering/      # AST → `CompiledFrontend` lowering (`include!` chain in `lowering_shim`)
│   │   │   ├── template.rs    # Template compatibility lowering helpers
│   │   │   └── tests.rs       # IR tests
│   │   ├── sla/               # Compiled .sla payload decoder
│   │   │   ├── mod.rs         # Public loader API
│   │   │   ├── packed.rs      # Packed element/parser constants
│   │   │   ├── symbols.rs     # Source files, spaces, symbol tables
│   │   │   ├── templates.rs   # ConstructTpl/OpTpl decode
│   │   │   ├── ghidra_const_tpl_map.rs # Ghidra SlaFormat const id ↔ SLA decoder parity tests
│   │   │   ├── pcode_golden_harness.rs # Golden p-code comparison harness (fixtures + schema)
│   │   │   ├── display.rs     # print/opprint/display metadata
│   │   │   └── tests.rs       # SLA decoder tests
│   │   ├── codegen.rs         # Deterministic generated artifact renderer
│   │   └── equivalence.rs     # Compiler-only bucketed equivalence report
│   └── runtime/
│       ├── mod.rs             # Public runtime facade, decode contracts, shared types
│       ├── frontend.rs        # Frontend construction, registry selection, native backend loading
│       ├── decode.rs          # Instruction/window decode contract handling
│       ├── lift.rs            # Instruction-to-function lifting orchestration
│       ├── diagnostics.rs     # Runtime trace/debug env ownership
│       ├── function.rs        # Raw p-code function lifting and CFG block construction
│       └── spine/             # Ghidra-style common runtime owners and compiled-table execution
│           └── compiled_table/ # Compiled-table decode/lift split by owner
│               ├── mod.rs     # Public decode entrypoints
│               ├── audit.rs   # SLA template feature audit (include! body)
│               ├── bind_decode.rs # Instruction length, cross-build bind-at-PC, context commits
│               ├── context.rs # Instruction/context bits
│               ├── strategy.rs # Native/common candidate strategy policy
│               ├── selection.rs # Decision traversal and terminal verification
│               ├── walker.rs  # include! hub → `walker/*.rs` (bind, structs, impl)
│               ├── handles.rs # Fixed/exported handle materialization
│               ├── token.rs   # Legacy shared-token cursor compatibility debt
│               ├── display.rs # Display rendering
│               ├── template_eval.rs # include! hub → `template_eval/*.rs` (flow, labels, types, emitter impl, trait)
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
- `runtime::frontend` owns frontend construction, registry selection, and native
  backend loading.
- `runtime::decode` owns instruction/window decode contract handling.
- `runtime::lift` owns instruction-to-function lifting orchestration.
- `runtime::diagnostics` owns runtime trace/debug env handling.
- `runtime::function` owns CFG reconstruction.
- `runtime::spine::compiled_table` is split by owner. `context`, `selection`,
  `strategy`, `walker`, `handles`, `display`, `audit`, `bind_decode`,
  `template_eval`, and `legacy_token_policy` should not grow cross-cutting
  orchestration logic (keep decode orchestration in `mod.rs`).

Future crate split boundary:

- compiler layers must not import runtime types.
- runtime layers may consume compiler facade/IR types but must not call compiler
  orchestration or generated-artifact writers.
- SLA decoder code may know IR template types only; it must not know runtime state.
- generated/native backend loading belongs in runtime frontend/registry ownership.
- Splitting this crate into `fission-sleigh-compiler` / `fission-sleigh-runtime` is
  optional: public paths (`compiler::ir::…`, `runtime::…`) and `fission-cli` wiring
  should be planned before moving workspace boundaries; compiled-table and IR
  `include!` hubs are intended to keep single-crate ownership cheap until then.

## Ghidra compiler pipeline mapping (12.0.4 reference)

Source of truth for behavior is under
[`vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling).
Fission does **not** link Ghidra; it mirrors invariants in Rust.

| Ghidra area | Primary Java entrypoints | Fission canonical owner |
|---------------|-------------------------|------------------------|
| Text preprocessor / lexer / parser | [`ghidra/sleigh/grammar/*`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/sleigh/grammar) | [`preprocessor.rs`](src/compiler/preprocessor.rs), [`ast.rs`](src/compiler/ast.rs) |
| Compile driver (.slaspec to .sla) | [`SleighCompileLauncher`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/pcodeCPort/slgh_compile/SleighCompileLauncher.java), [`SleighCompile`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/pcodeCPort/slgh_compile/SleighCompile.java) | Ghidra binary `.sla` checked in / install path; Fission consumes artifacts via [`sla/`](src/compiler/sla/) |
| Packed element / attribute ids | [`SlaFormat`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/pcode/utils/SlaFormat.java) | [`sla/packed.rs`](src/compiler/sla/packed.rs) + parity tests in [`sla/ghidra_const_tpl_map.rs`](src/compiler/sla/ghidra_const_tpl_map.rs) |
| Operand symbols (reloffset, minlen, code, …) | [`OperandSymbol`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/pcodeCPort/slghsymbol/OperandSymbol.java) encode/decode | [`sla/symbols.rs`](src/compiler/sla/symbols.rs) → `CompiledOperandSpec` / handle metadata |
| ConstructTpl / const decode | `SleighCompile` + pcode encode | [`sla/templates.rs`](src/compiler/sla/templates.rs) |
| Post-merge decode metadata | [`SleighInstructionPrototype.resolve`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/SleighInstructionPrototype.java) + [`OperandSymbol`](../../../vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/SoftwareModeling/src/main/java/ghidra/app/plugin/processors/sleigh/symbol/OperandSymbol.java) (MINLEN/CODE on `.sla`) | [`decode_metadata.rs`](src/compiler/decode_metadata.rs) — `cursor_policy_bits` mirror Ghidra x86 **subtable symbol names** (same identifiers as the compiled Sleigh language); `RELATIVE_TRAILING` also OR’d from `ATTRIB_CODE` on operands. No `entry_id` / arch string gates in the walker. |
| Runtime decode / emit | `SleighParserContext`, `PcodeEmit`, … | [`runtime/spine/compiled_table/`](src/runtime/spine/compiled_table/), [`runtime/spine/emitter.rs`](src/runtime/spine/emitter.rs) |

**Non-goals (avoid duplicate / endless port):** porting the full Antlr grammar to Rust, reimplementing the entire `SleighCompile` pass in-tree, or linking `vendor/ghidra` into the crate. When `.sla` lacks a fact Ghidra keeps only in Java memory, extend the **SLA decoder** from `SlaFormat` / `OperandSymbol.encode` evidence—do not approximate in the runtime walker.

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

## Anti-patterns (do not)

### Boundaries and surfaces

- Do not add CLI-, benchmark-, or UI-layer hacks to compensate for decode/lift
  bugs; fix the canonical owner under `src/compiler/` or `src/runtime/spine/`.
- Do not add silent success paths when decode/runtime execution is unsupported;
  return typed errors and keep behavior deterministic for snapshot/automation
  consumers.
- Do not hardcode binary-specific one-off rules without invariant-based guards
  (CFG/dom facts, spec-derived metadata, or documented Ghidra parity evidence).

### Specs, imports, and repo layout

- Do not use `vendor/rsleigh/slaspec` as the canonical spec import source when
  refreshing files; follow `specs/README.md` (Ghidra
  `Processors/*/data/languages/` → `specs/languages/<Processor>/`).
- Do not describe `specs/languages/` as “Sleigh-only”: the mirror may include
  sibling artifacts (for example `.dwarf`, `*.register.info`, `old/`) that are
  not inputs to the in-tree Sleigh compiler path unless explicitly wired.
- Do not claim “all Ghidra language files exist only under `specs/`” when
  auditing the whole repo; other trees (for example `vendor/ghidra`, workspace
  `utils/`) may hold additional curated or reference data—state which root is
  authoritative for the task.

### Ghidra parity and references

- Do not treat “Java line-by-line migration” as the default plan; prefer a
  declared equivalence surface (instruction length, disassembly, normalized
  p-code, etc.) plus golden or differential checks against a fixed Ghidra version
  and spec snapshot.
- Do not approximate semantics in the runtime walker when the contract can be
  grounded in `.sla` / `SlaFormat` / `OperandSymbol` evidence; extend the SLA
  decoder or compiler IR instead (see non-goals under
  [Ghidra compiler pipeline mapping](#ghidra-compiler-pipeline-mapping-1204-reference)).
- Do not link `libsla` / `libsla-sys`, Ghidra JVM bytecode, or JNI into this crate
  without an explicit product decision; `vendor/libsla-sys` and
  `vendor/ghidra/.../decompile/cpp` are **reference** for behavior, not an
  undeclared runtime dependency.

### Runtime structure

- Do not reintroduce removed per-arch shims as new “helpers”, “providers”,
  “quirks”, or “text” modules; keep transitional behavior behind
  `legacy_token_policy` and document it as debt.
- Do not duplicate `compiler::policy` language selection or alias rules inside
  runtime modules.

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
