# 2026-04-24 Changelog

## Summary

This wave narrowed the `compiled_table.rs` compatibility holdout by promoting two
previously blocked x86-64 constructor families into the common SLEIGH runtime path:

- 32-bit destination constructors guarded by:
  - `check_Reg32_dest`
  - `check_Rmr32_dest`
  - `check_rm32_dest`
  - `check_EAX_dest`
- `MOVSXD` constructors gated by the blanket `bit64=1` runtime constraint

The immediate goal was not full benchmark recovery. The direct goal was to remove a
real `DecodeNoMatch` family inside the shared runtime spine without reintroducing a
bridge, fake p-code, or architecture-specific formatter/emitter layers.

## 1. Compiler Gate Reduction

### Scope

- canonical owner:
  - `crates/fission-sleigh/src/compiler/ir.rs`
- reference model:
  - `DecisionNode`
  - `SleighParserContext`
  - `ConstructState`
  - `ParserWalker`
  - `ConstructTpl`
  - `PcodeEmit`

### What changed

- `unsupported_template_reason(...)` no longer rejects every constructor that contains
  `check_*`.
- a narrow typed allowlist now keeps these runtime constraints executable:
  - `check_Reg32_dest`
  - `check_Rmr32_dest`
  - `check_rm32_dest`
  - `check_EAX_dest`
- the blanket `bit64=1` reject path was removed.
  - this reopened the `MOVSXD` constructor family for the checked-in x86-64 entry
- all other unknown `check_*` constraints still fail closed as:
  - `unsupported_runtime_constraint`

### Result

Checked-in generated inventory changed accordingly:

- `crates/fission-sleigh/generated/compiler_manifest.json`
- `crates/fission-sleigh/generated/x86/x86-64/parsed_inventory.json`
- `crates/fission-sleigh/generated/x86/x86/parsed_inventory.json`

For `x86-64`, unsupported executable-template count dropped again:

- before this wave: `29`
- after adding `MOVSXD`: `27`

## 2. Runtime Semantics Correction

### Scope

- canonical owner:
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`

### What changed

- register writes to 32-bit general-purpose destinations now also materialize the
  canonical 64-bit register state through zero-extension.
- this closes a real semantic gap behind the `check_*_dest` constructors.
- the common compiled-table executor now has targeted regression tests for:
  - `lea` into a 32-bit destination
  - RIP-relative `mov` into a 32-bit destination
  - `movsxd`
  - 32-bit write zero-extension into the canonical 64-bit register

### Result

The following previously failing rows now decode and lift through `rust_sleigh`
without `DecodeNoMatch`:

- `test_functions.exe:add @ 0x140001450`
- `test_functions.exe:__main @ 0x1400019c0`
- `test_functions.exe:_FindPESection @ 0x140002600`

The remaining explicit decode failure in the single-binary gate is now narrowed to:

- `test_functions.exe:_fpreset @ 0x1400025c0`
  - first instruction: `fninit`
  - still fail-closed as `DecodeNoMatch`

## 3. Validation

### Required Rust validation

- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo test -p fission-sleigh generated_runtime_decodes_reg32_lea_without_decode_no_match -- --nocapture`
  - result: passed
- `cargo test -p fission-sleigh generated_runtime_decodes_movsxd_without_decode_no_match -- --nocapture`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `42 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed

### Runtime smoke

- `target/release/fission_cli disasm ... --addr 0x140001450 --count 8`
  - result: decoded via `rust_sleigh`
- `target/release/fission_cli decomp ... --addr 0x1400019c0 --json`
  - result: succeeds via `rust_sleigh`, no fallback
- `target/release/fission_cli disasm ... --addr 0x140002600 --count 8`
  - result: `movsxd` row now decodes via `rust_sleigh`
- `target/release/fission_cli decomp ... --addr 0x140002600 --json`
  - result: succeeds via `rust_sleigh`, no fallback

## 4. Benchmark Readout

### Single-binary benchmark

- baseline:
  - `benchmark/artifacts/full_benchmark/windows-small-c-full-processor-runtime-no-iced-latest/test-functions`
- previous broken load-spec/regression state:
  - `29.00%`
- after the first `check_*_dest` promotion:
  - `37.45%`
- after reopening `MOVSXD`:
  - `37.00%`

Current gate remains failed against the `44.56%` baseline.

Current notable deltas:

- `both_success_rate`
  - baseline: `100.000%`
  - current: `98.000%`
- explicit decode failures:
  - before this wave subset: `2`
  - after `MOVSXD`: `1`
- `unsupported_indirect_control_count`
  - baseline: `0`
  - current: `2`
- `owner_materialization_stabilized`
  - baseline: `30`
  - current: `812`
- row fidelity gate:
  - still failing at `0x140001010`

### Interpretation

This wave removed a real `DecodeNoMatch` family, but the benchmark result shows the
remaining owner is deeper than constructor admission alone.

What improved:

- `DecodeNoMatch` is gone on `0x140001450`
- `DecodeNoMatch` is gone on `0x1400019c0`
- `DecodeNoMatch` is gone on `0x140002600`
- single-binary explicit error count dropped from `2` to `1`
- single-binary similarity recovered far above the earlier `29.00%` regression state

What remains:

- `fninit` / x87-family coverage is still missing
- startup-quality drift remains visible at `0x140001010`
- materialization/shape drift is still far from baseline

## 5. Next Owner

The next direct owner remains the shared runtime spine, not loader routing:

- `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`

Priority order:

1. remove the remaining `_fpreset` `DecodeNoMatch` family without fake p-code
2. narrow the `0x140001010` startup drift
3. reduce `materialization_stabilized` and generic-name drift before claiming parity

This wave intentionally did not reintroduce:

- `iced-x86`
- architecture-specific runtime helper modules
- fake text formatting
- fake p-code for unsupported semantics

## 6. Raw P-code Parity Benchmark Harness

Added a new raw P-code parity benchmark under:

- `benchmark/raw_p_code_benchmark`

The harness compares Fission's raw SLEIGH runtime output against Ghidra 12.0.4
raw instruction P-code.

### Owner Contract

- Ghidra oracle path uses PyGhidra and `Instruction.getPcode()`.
- Fission path uses `RuntimeSleighFrontend::new_for_load_spec(...)` and
  `decode_and_lift_with_len(...)`.
- The comparator works at the raw instruction P-code layer, not decompiler
  `HighFunction` output.
- Output buckets include decode errors, length mismatches, opcode mismatches,
  arity mismatches, varnode space mismatches, and varnode size mismatches.

### Added Files

- `crates/fission-sleigh/examples/raw_pcode_probe.rs`
- `benchmark/raw_p_code_benchmark/ghidra_raw_pcode.py`
- `benchmark/raw_p_code_benchmark/fission_raw_pcode.py`
- `benchmark/raw_p_code_benchmark/compare_raw_pcode.py`
- `benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py`
- `benchmark/raw_p_code_benchmark/README.md`

### Smoke Results

`test_functions.exe:add @ 0x140001450`

- command: `run_raw_pcode_parity.py --addr 0x140001450 --count 2`
- result:
  - `pcode_op_count_mismatch = 2`
  - `pcode_opcode_mismatch = 2`

`test_functions.exe:_fpreset @ 0x1400025c0`

- command: `run_raw_pcode_parity.py --addr 0x1400025c0 --count 2`
- result:
  - `decode_no_match = 1`
  - `missing_fission_instruction = 1`

These are expected first-read results: the harness is now able to show both
shape drift and the remaining `_fpreset` decode hole at the raw SLEIGH layer.

### Validation

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
  - result: passed
- `cargo run -p fission-sleigh --example raw_pcode_probe -- ...`
  - result: emitted JSON raw P-code for `0x140001450`
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo check -p fission-cli`
  - result: passed

### Next Owner

Use this harness before full decompiler benchmark runs to close:

- `_fpreset` / x87 decode no-match
- startup raw P-code opcode sequence drift
- varnode shape and unique-temp shape mismatches
