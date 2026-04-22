# 2026-04-23 Changelog

## Summary

This changelog records the `fission-sleigh` full-architecture mirror wave that moved the
checked-in SLEIGH spec/compiler/runtime-skeleton surface from a limited local family set to
the full Ghidra 12.0.4 Processor/language inventory.

Current checked-in state:

- processors mirrored: `38`
- `.slaspec` variants mirrored: `146`
- canonical spec manifest:
  - `crates/fission-sleigh/specs/ghidra_language_manifest.json`
- canonical compiler manifest:
  - `crates/fission-sleigh/generated/compiler_manifest.json`

This wave is a compiler/runtime-skeleton expansion wave, not an executable parity wave.

## 1. Full Ghidra SLEIGH Architecture Mirror

### Scope

- source of truth:
  - `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Processors/*/data/languages`
- canonical owner:
  - `crates/fission-sleigh/`
- public behavior:
  - full Processor/language discovery and runtime skeleton registration
- non-goal:
  - multi-ISA executable parity in this wave

### What changed

- `crates/fission-sleigh/specs/languages/` now mirrors Ghidra Processor names 1:1
  instead of using Fission-local buckets like `aarch64`, `arm32`, `powerpc`.
- full mirror coverage is now:
  - `38` processors
  - `146` `.slaspec` variants
- nested helper/include trees are preserved during import.
  - this was required for processors like `V850`
- new canonical spec manifest:
  - `crates/fission-sleigh/specs/ghidra_language_manifest.json`
- compiler manifest now reports full-set variant coverage with typed compile status:
  - `crates/fission-sleigh/generated/compiler_manifest.json`
- compiler discovery no longer depends on the old hardcoded 6-family ordering.
- runtime registry lookup now supports:
  - exact entry stem
  - exact entry spec
  - derived language ids from `.ldefs` when present
  - compatibility aliases such as `aarch64`, `arm32`, `powerpc`, `riscv`

### Parser / import fixes found during rollout

Two full-set blockers were exposed and fixed while expanding to the full Ghidra set:

- `TI_MSP430X.slaspec`
  - the constructor parser counted `{` / `}` inside quoted display text as structural braces
  - fixed by making brace counting structural instead of raw-character based
- `V850.slaspec`
  - the initial import missed nested `Helpers/` and `Instructions/` trees
  - fixed by recursive mirror import

### Runtime status

- `x86-64` remains the only `ExecutableCandidate`
- all other variants are now registered as compile-only skeletons
- `iced-x86` is still retained as the x86-64 oracle/bridge until parity deletion gates pass

### Validation

- `cargo check -p fission-sleigh`
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `37 passed / 0 failed`
- `cargo check -p fission-cli`

### Result

This wave completes the first-stage definition of support for the full Ghidra 12.0.4
architecture set inside `fission-sleigh`:

- full spec mirror
- full compile coverage
- full runtime registry skeleton
- typed fail-closed unsupported behavior

The next owner remains executable runtime parity, starting with:

1. `x86-64`
2. `AARCH64`
3. `ARM`
4. `MIPS`
5. `PowerPC`
6. `RISCV`

## 2. x86-64 Generated Runtime Parity Wave 1

### Scope

- reference spine:
  - `DecisionNode`
  - `ConstructState`
  - `ParserWalker`
  - `ConstructTpl`
  - `PcodeEmit`
- canonical owner:
  - `crates/fission-sleigh/src/compiler/ir.rs`
  - `crates/fission-sleigh/src/runtime/generated_x86_64.rs`
- env gate:
  - `FISSION_ENABLE_GENERATED_X86_64_RUNTIME=1`

### What changed

- decision-tree selection is no longer bucket-only.
  - added a global root traversal before legacy bucket fallback
  - compiler IR now supports instruction-byte probes in addition to:
    - `operand_size_code`
    - `mod_bits`
    - `reg_opcode`
- generated runtime now records a constructor-tree working state instead of only a flat handle list.
  - `RuntimeConstructState` now carries `construct_nodes`
  - operand decode is recorded as parent/child construct nodes for future `ParserWalker` expansion
- fixed a real x86-64 semantic bug:
  - RIP-relative effective addresses now use `inst_next + displacement`
  - previous generated path used `inst_start + displacement`
  - this directly affected startup/runtime-heavy rows
- generated artifact contract was updated additively:
  - `decision_tree.root_node_index`
  - structured instruction-byte probe rendering

### Validation

- targeted runtime tests:
  - `generated_runtime_decodes_startup_rip_relative_load`
  - `generated_runtime_records_decision_trace_for_startup_store`
- crate validation:
  - `cargo check -p fission-sleigh`
  - `cargo test -p fission-sleigh -- --test-threads=1`
  - `cargo check -p fission-cli`
  - `cargo build -p fission-cli --release`
- generator validation:
  - `cargo run -p fission-sleigh --example generate_sleigh_frontends`

### Benchmark readout

Limited same-axis benchmark:

- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-sleigh-x86-runtime-latest`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-sleigh-decisiontree-parserwalker-v2-latest`

Observed result:

- weighted average normalized similarity:
  - `44.47% -> 44.56%`
- generated runtime smoke:
  - `WinMainCRTStartup @ 0x1400013e0`
  - `mainCRTStartup @ 0x140001400`
  - `fibonacci @ 0x140001470`
  all decoded through `engine_used=rust_sleigh`, `fell_back=false`
- release promotion:
  - still `false`
- current interpretation:
  - catastrophic decode/lift regressions are not visible in the limited gate
  - deletion gate for `iced-x86` remains closed

### Result

This wave moved the x86-64 generated path closer to the Ghidra runtime spine without deleting the
bridge/oracle path.

What improved:

- generated selection can now traverse a global decision graph
- RIP-relative addressing is semantically corrected
- checked-in generated artifacts encode the richer decision-tree contract
- limited benchmark recovered to the `44.56%` class with no crash rows

What remains:

- `ConstructTpl` coverage is still partial
- `ParserWalker` state exists, but recursive operand/subtable expansion is not complete
- `iced-x86` deletion gate remains closed until raw parity buckets narrow further
