# fission-sleigh

`fission-sleigh` is Fission's Rust-native Sleigh lifting crate.
It resolves local `.slaspec` files from architecture-organized spec folders and lifts instruction bytes into `fission-pcode` operations without a runtime Sleigh dependency.
It now also owns a clean-room compiler-only Sleigh front-end wave for deterministic spec preprocessing, AST/inventory compilation, generated artifact emission, and non-runtime x86-64 equivalence reporting.

## What this crate owns

- Local language/spec path resolution from `specs/languages/<arch>/*.slaspec`
- Instruction-level decode + lift (`decode_and_lift`, `decode_and_lift_with_len`)
- Function-level lifting contract with stop reason metadata
- Basic block reconstruction from p-code control flow (`build_cfg_blocks`)
- Compiler-only Sleigh front-end spine (`compiler/`)
  - tokenize
  - preprocess (`@include`, `@define`, conditional guards)
  - parse (constructor / macro / with-block AST)
  - compile (inventory + pattern graph + semantic action IR)
  - deterministic codegen into `generated/x86/`
- Architecture paths:
  - AArch64 semantic/control lifting
  - x86 length/control/semantic lifting

## Public API surface

Primary entrypoint:

- `SleighLifter`

Supporting types/functions:

- `LiftedPcodeFunction`
- `LiftStopReason`
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
│   └── lifter/
│       ├── mod.rs
│       ├── common.rs
│       ├── arm32/
│       │   └── README.md      # planned scaffold
│       ├── mips/
│       │   └── README.md      # planned scaffold
│       ├── riscv/
│       │   └── README.md      # planned scaffold
│       ├── powerpc/
│       │   └── README.md      # planned scaffold
│       ├── aarch64/
│       │   ├── mod.rs
│       │   ├── control.rs
│       │   └── semantic.rs
│       └── x86/
│           ├── mod.rs
│           ├── length.rs
│           ├── control.rs
│           ├── semantic.rs
│           └── semantic/
│               ├── alu.rs
│               ├── addressing.rs
│               └── tests.rs
├── specs/
│   └── languages/
│       ├── aarch64/
│       ├── arm32/
│       ├── mips/
│       ├── powerpc/
│       ├── riscv/
│       └── x86/
└── generated/
    └── x86/
```

## Quick usage

```rust
use fission_sleigh::lifter::SleighLifter;

fn main() -> anyhow::Result<()> {
    // Example language names available in specs/languages/<arch>/:
    // - "x86-64"
    // - "AARCH64"
    let lifter = SleighLifter::new_for_language("x86-64")?;

    let bytes = [0x90, 0xC3]; // nop; ret
    let address = 0x401000;

    // Single-instruction decode + semantic/control lift
    let (ops, decoded_len) = lifter.decode_and_lift_with_len(&bytes, address)?;
    println!("decoded_len={decoded_len}, ops={}", ops.len());

    // Function-level lifting with contract metadata
    let lifted = lifter.lift_raw_pcode_function_with_contract(&bytes, address, 512)?;
    println!(
        "decoded_instructions={}, stop_reason={:?}, blocks={}",
        lifted.decoded_instructions,
        lifted.stop_reason,
        lifted.function.blocks.len()
    );

    Ok(())
}
```

## Spec resolution behavior

- `SleighLifter::new_for_language("<name>")` looks for:
  - `crates/fission-sleigh/specs/languages/**/<name>.slaspec`
- The checked-in spec tree is mirrored from:
  - `vendor/ghidra/ghidra_12.0.4_PUBLIC/Ghidra/Processors/*/data/languages/`
- `SleighLifter::new(path)` infers language name from the file stem.

## Validation

From repository root:

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_x86_frontend
```

When changes may affect decompilation routing behavior:

```bash
cargo check -p fission-cli
```

## Notes

- This crate intentionally avoids a runtime Sleigh engine dependency.
- Outputs are designed to be deterministic for the same input bytes/address.
- Semantic correctness fixes should be made in this crate (not CLI/UI layers).
- The first clean-room migration consumer is `x86-64.slaspec`; generated front-end output is checked in under `crates/fission-sleigh/generated/x86/` but is not yet the canonical runtime decoder path.
