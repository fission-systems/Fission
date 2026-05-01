# fission-sleigh

`fission-sleigh` is Fission's Rust-native Sleigh front-end crate.
It resolves local `.slaspec` files from a Ghidra-mirrored processor tree, compiles all checked-in variants into deterministic generated artifacts, and owns the new fail-closed compiled runtime registry.
The previous architecture-specific hand-lifter path has been removed.

## What this crate owns

- Local language/spec path resolution from `utils/sleigh-specs/languages/<Processor>/**/*.slaspec`
- Runtime registry and decode/lift contracts under `runtime/`
- Typed fail-closed errors for generated front-ends that are not executable yet
- Basic block reconstruction from p-code control flow (`build_cfg_blocks`)
- Generic Sleigh front-end spine (`compiler/`)
  - tokenize
  - preprocess (`@include`, `@define`, conditional guards)
  - parse (constructor / macro / with-block AST)
  - compile (inventory + pattern graph + semantic action IR)
  - deterministic codegen into `generated/<Processor>/<entry-spec-stem>/`
- Runtime statuses:
  - `x86-64`: `ExecutableCandidate`, but p-code template execution is still incomplete
  - all other variants: `RegisteredCompileOnly`
- Current mirror coverage:
  - `38` Ghidra processors
  - `146` checked-in `.slaspec` variants
  - canonical processor/variant manifest: `utils/sleigh-specs/ghidra_language_manifest.json`

## Public API surface

Primary entrypoint:

- `RuntimeSleighFrontend`

Supporting types/functions:

- `DecodedPcodeFunction`
- `DecodeContract`
- `DecodeStopReason`
- `CompiledRuntimeRegistry`
- `build_cfg_blocks`
- `is_terminal_control_flow`

## Current structure

```text
crates/fission-sleigh/
├── src/
│   ├── lib.rs
│   ├── compiler/
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   ├── preprocessor.rs
│   │   ├── ast.rs
│   │   ├── ir.rs
│   │   ├── codegen.rs
│   │   └── equivalence.rs
│   └── runtime/
│       ├── mod.rs
│       ├── spine/
│       │   ├── context.rs
│       │   ├── decision.rs
│       │   ├── construct.rs
│       │   ├── walker.rs
│       │   ├── template.rs
│       │   └── emitter.rs
│       ├── processors/
│       │   ├── aarch64/
│       │   ├── arm/
│       │   ├── mips/
│       │   ├── powerpc/
│       │   ├── riscv/
│       │   ├── ...
│       │   └── x86/
│       │       └── generated.rs
└── generated/
    ├── compiler_manifest.json
    ├── AARCH64/
    ├── ARM/
    ├── MIPS/
    ├── PowerPC/
    ├── RISCV/
    ├── ...
    └── x86/
```

## Quick usage

```rust
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn main() -> anyhow::Result<()> {
    // Example language names available in utils/sleigh-specs/languages/<Processor>/:
    // - "x86-64"
    // - "AARCH64"
    // - "AARCH64:LE:64:v8A" (if derivable from checked-in .ldefs)
    let runtime = RuntimeSleighFrontend::new_for_language("x86-64")?;
    println!("status={}", runtime.status().as_str());

    let bytes = [0x90, 0xC3]; // nop; ret
    let address = 0x401000;

    let (ops, len) = runtime.decode_and_lift_with_len(&bytes, address)?;
    assert_eq!(len, 2);
    assert!(!ops.is_empty());

    Ok(())
}
```

## Spec resolution behavior

- `RuntimeSleighFrontend::new_for_language("<name>")` looks for:
  - exact entry-spec stem
  - exact derived language id when present in `utils/sleigh-specs/ghidra_language_manifest.json`
  - compatibility aliases like `aarch64`, `arm32`, `powerpc`, `riscv`
- Spec root resolution order:
  - `FISSION_SLEIGH_SPEC_DIR`
  - repo-relative `utils/sleigh-specs`
- The checked-in spec snapshot is mirrored from:
  - `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Processors/*/data/languages/`
- `RuntimeSleighFrontend::new(path)` infers language name from the file stem.

## Ghidra clean-room runtime spine

The generated runtime is organized around Ghidra's SLEIGH execution ownership,
but implemented as dependency-free Rust:

| Ghidra owner | Fission owner |
|---|---|
| `SleighLanguage` | `RuntimeSleighFrontend` plus compiled language registry |
| `SleighParserContext` | `runtime::spine::RuntimeInstructionContext` |
| `DecisionNode` | `CompiledDecisionTree` plus `runtime::spine::DecisionProbeEvaluator` |
| `ConstructState` | `runtime::spine::RuntimeConstructState` |
| `ParserWalker` | `runtime::spine::RuntimeParserWalker` |
| `ConstructTpl` | compiler-produced constructor templates |
| `PcodeEmit` | `runtime::spine::RuntimePcodeEmitter` |

Processor-specific runtime modules may extract ISA fields such as prefixes,
ModRM/SIB, context bits, address spaces, and register mappings. They must not
own semantic repair or mnemonic-level p-code policy; that belongs in the shared
spine and compiler-produced templates.

Runtime processor folders are checked in for all `38` mirrored Ghidra processors.
Only `x86` is an executable candidate today; the remaining processor modules are
typed compile-only skeletons until their generated runtime parity gates are implemented.

## Validation

From repository root:

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_sleigh_frontends
```

When changes may affect decompilation routing behavior:

```bash
cargo check -p fission-cli
```

## Notes

- This crate intentionally avoids both a runtime Sleigh engine dependency and
  temporary decode bridges inside `fission-sleigh`.
- Outputs are designed to be deterministic for the same input bytes/address.
- Semantic correctness fixes should be made in this crate (not CLI/UI layers).
- The clean-room compiler consumer now preprocesses/parses/compiles/codegens all checked-in `.slaspec` variants with one generic compiler API.
- Generated front-end output is checked in under `crates/fission-sleigh/generated/<Processor>/<entry-spec-stem>/`.
- The runtime registry consumes the generated/spec inventory shape, but semantic p-code template execution is not complete yet.
- The current all-variant manifest is `crates/fission-sleigh/generated/compiler_manifest.json`.
