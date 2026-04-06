# fission-sleigh

`fission-sleigh` is Fission's Rust-native Sleigh lifting crate.
It resolves local `.slaspec` files and lifts instruction bytes into `fission-pcode` operations without a runtime Sleigh dependency.

## What this crate owns

- Local language/spec path resolution from `specs/languages/*.slaspec`
- Instruction-level decode + lift (`decode_and_lift`, `decode_and_lift_with_len`)
- Function-level lifting contract with stop reason metadata
- Basic block reconstruction from p-code control flow (`build_cfg_blocks`)
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
└── specs/
    └── languages/
```

## Quick usage

```rust
use fission_sleigh::lifter::SleighLifter;

fn main() -> anyhow::Result<()> {
    // Example language names available in specs/languages:
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
  - `crates/fission-sleigh/specs/languages/<name>.slaspec`
- `SleighLifter::new(path)` infers language name from the file stem.

## Validation

From repository root:

```bash
cargo check -p fission-sleigh
cargo test -p fission-sleigh
```

When changes may affect decompilation routing behavior:

```bash
cargo check -p fission-cli
```

## Notes

- This crate intentionally avoids a runtime Sleigh engine dependency.
- Outputs are designed to be deterministic for the same input bytes/address.
- Semantic correctness fixes should be made in this crate (not CLI/UI layers).
