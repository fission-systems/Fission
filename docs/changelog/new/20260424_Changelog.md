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

## 7. Ghidra `.sla` ConstructTpl Payload Decoder

### Scope

- canonical owner:
  - `crates/fission-sleigh/src/compiler/sla.rs`
  - `crates/fission-sleigh/src/compiler/ir.rs`
  - `crates/fission-sleigh/src/runtime/spine/template.rs`
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`
- reference structure:
  - Ghidra 12.0.4 `PackedDecode`
  - Ghidra 12.0.4 `ConstructTpl`
  - Ghidra 12.0.4 `ConstTpl`
  - Ghidra 12.0.4 `VarnodeTpl`
  - Ghidra 12.0.4 `HandleTpl`
  - Ghidra 12.0.4 `OpTpl`
  - Ghidra 12.0.4 `PcodeEmit`

### What changed

- Added a real packaged `.sla` decoder for the Ghidra packed zlib payload.
- Decoded source files, spaces, constructors, `ConstructTpl`, `OpTpl`, `VarnodeTpl`, `HandleTpl`, and `ConstTpl`.
- Promoted Fission executable template IR toward Ghidra-shaped owners:
  - `CompiledConstTpl`
  - `CompiledSpaceTpl`
  - `CompiledVarnodeTpl::Varnode`
  - `CompiledHandleTpl`
  - `CompiledOpTpl`
- Overlaid decoded `.sla` templates onto Fission constructors by exact source-file basename and line.
- Preserved original source line numbers inside `with` blocks so `.sla` constructor line mapping does not drift.
- Kept raw P-code execution fail-closed:
  - `RuntimeTemplateEvaluator` only executes `SpecDerived`.
  - `CompatibilityLowered` cannot produce raw P-code success.
  - compatibility varnodes inside `SpecDerived` are rejected.
  - unsupported userops, `BUILD`, dynamic handles, unresolved labels, and unsupported spaces stop the whole instruction.
- Fixed an existing pattern parser bug where `row=` matched inside `frow=`, causing x87 rows to collide with unrelated `0xC7` rows.

### Raw P-code gate

New report:

- `benchmark/artifacts/raw_p_code_benchmark/construct_tpl_payload_decoder/aggregate_raw_pcode_parity_report.json`

Current totals:

- `row_count = 17`
- `compat_emitter_used_total = 0`
- `invalid_pcode_shape = 0`
- `fake_placeholder_op = 0`
- `template_source_totals.SpecDerived = 4`
- `unsupported_template = 15`
- `missing_fission_instruction = 27`
- `varnode_space_mismatch = 4`
- `input_varnode_mismatch = 3`
- `ghidra_decode_error = 1`

Performance readout:

- Fission wall clock: `16.305884669010993s`
- Ghidra wall clock: `40.82362453988753s`
- Fission over Ghidra wall-clock speedup: `2.503612981972822x`
- Fission emitted `4` instructions and `19` raw P-code ops in this strict gate.
- Ghidra emitted `44` instructions and `162` raw P-code ops.

Interpretation:

- The raw P-code success path is now strict: no approximate compatibility P-code is counted as success.
- The first real successes are `SpecDerived` rows, including `RET` and `FNINIT`-adjacent rows.
- Remaining failures are now honest owner buckets:
  - unsupported `.sla` template primitives
  - missing Fission instruction after fail-closed unsupported
  - varnode identity/space naming drift

### Architecture readiness regression guard

New report:

- `benchmark/artifacts/full_benchmark/architecture_readiness_construct_tpl_payload_decoder/architecture_readiness_aggregate.json`

Result:

- `row_count = 53`
- `disasm_ready = 2`
- `inventory_ready = 51`
- `load_failed = 0`
- `selection_compile_only = 12`
- `disasm_runtime_failure = 39`

Compared to `architecture_readiness_next_wave`, there were no readiness regressions.

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `54 passed / 0 failed`
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
  - result: passed
- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir /Users/sjkim1127/Fission/vendor/ghidra/ghidra_12.0.4_PUBLIC --output-dir benchmark/artifacts/raw_p_code_benchmark/construct_tpl_payload_decoder --fission-release`
  - result: passed, strict no-approx invariants preserved
- `python3 benchmark/full_benchmark/run_architecture_readiness_parallel.py --manifest benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json --baseline-report benchmark/artifacts/full_benchmark/architecture_readiness_next_wave/architecture_readiness_aggregate.json --output-dir benchmark/artifacts/full_benchmark/architecture_readiness_construct_tpl_payload_decoder`
  - result: passed, no readiness regression

### Next owner

The next direct owner is still not display text or decompiler postprocess. It is the remaining `ConstructTpl` execution support:

- `HandleTpl` resolution through `RuntimeParserWalker`
- userop and `CALLOTHER` identity
- `BUILD`/subconstructor execution
- label and relative const resolution
- Ghidra space-name preservation in raw P-code comparison

Until those are implemented, unsupported rows must remain typed `UnsupportedPcodeTemplate`.

## 8. Raw P-code Performance Metrics

The raw P-code parity harness now records speed metrics in addition to
correctness buckets.

### What changed

- `benchmark/raw_p_code_benchmark/ghidra_raw_pcode.py`
  - records end-to-end oracle wall-clock time
  - records instruction count and raw p-code op count
- `benchmark/raw_p_code_benchmark/fission_raw_pcode.py`
  - records end-to-end probe wall-clock time
  - records instruction count and raw p-code op count
- `benchmark/raw_p_code_benchmark/compare_raw_pcode.py`
  - adds a `performance` section with:
    - `ghidra`
    - `fission`
    - `delta`
- `benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py`
  - adds manifest-level `performance_summary`

### Metric semantics

These are harness-level timings, not isolated decode-only microbenchmarks.

Tracked fields:

- `wall_clock_sec`
- `instruction_count`
- `pcode_op_count`
- `instructions_per_sec`
- `pcode_ops_per_sec`
- `wall_clock_speedup_fission_over_ghidra`

### Smoke

- command:
  - `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001453 --count 1 --language x86:LE:64:default --compiler windows --ghidra-dir /Users/sjkim1127/Downloads/ghidra_12.0.4_PUBLIC --output-dir benchmark/artifacts/raw_p_code_benchmark/raw_speed_smoke --fission-release`
- result:
  - correctness buckets still report the expected mismatch on this row
  - performance section is emitted successfully for both tools

## 9. Architecture Readiness Parallel Lane

Added a parallel architecture-readiness runner over the LLVM baremetal smoke corpus.

### Added files

- `benchmark/full_benchmark/run_architecture_readiness_parallel.py`
- `benchmark/binary/LLVM_SMOKE_README.md` updated with runner usage

### Lane semantics

This lane is intentionally lower-bar than raw P-code parity.

It checks, per binary:

- `info --json`
- `list --json`
- `disasm --addr <first_function_or_entry> --count 4 --json`

and classifies each row into:

- `disasm_ready`
- `inventory_ready`
- `metadata_ready`
- `load_failed`

### LLVM baremetal corpus run

- manifest:
  - `benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json`
- row count:
  - `53`
- aggregate report:
  - `benchmark/artifacts/full_benchmark/architecture_readiness_llvm53/architecture_readiness_aggregate.json`

### Current totals

- `info_ok = 21`
- `list_ok = 21`
- `disasm_ok = 1`

Readiness classes:

- `disasm_ready = 1`
- `inventory_ready = 20`
- `load_failed = 32`

### Interpretation

This separates "broad architecture loading/inventory coverage" from
"raw-pcode/runtime parity coverage".

Current signal:

- broad loader/inventory readiness exists beyond x86-64
- many architectures still stop at compile-only runtime selection or loader gaps
- only `x86-64` currently clears the full readiness lane through disasm in this smoke corpus

## 10. LLVM 53 Architecture Readiness Promotion

This wave raised the architecture-readiness lane from a coarse "load failed"
report into a layered owner breakdown over the checked-in LLVM baremetal smoke
corpus.

### Direct owners

- `crates/fission-core/src/architecture.rs`
- `crates/fission-sleigh/src/compiler/mod.rs`
- `benchmark/full_benchmark/run_architecture_readiness_parallel.py`

### What changed

- ELF load-spec selection now includes additional machine families seen in the
  current LLVM smoke corpus:
  - RISC-V
  - MIPS / MIPS R6
  - PowerPC / PowerPC64
  - SPARC V9
  - eBPF
  - LoongArch
- x86 32-bit was promoted from `registered_compile_only` to
  `executable_candidate` in the checked-in Ghidra language manifest generation.
- readiness aggregate output now records:
  - `failure_bucket`
  - `failure_owner`
  - `seed_source`
  - `failure_bucket_totals`
  - `failure_owner_totals`
- disasm seed order is now fixed as:
  - first listed function
  - entry point
  - first executable section start

### LLVM 53 lane result after this wave

- aggregate:
  - `benchmark/artifacts/full_benchmark/architecture_readiness_llvm53_after_release/architecture_readiness_aggregate.json`
- row count:
  - `53`
- stage totals:
  - `info_ok = 53`
  - `list_ok = 53`
  - `disasm_ok = 2`
- readiness totals:
  - `disasm_ready = 2`
  - `inventory_ready = 51`

### Failure buckets after promotion

- `selection_compile_only = 19`
- `disasm_runtime_failure = 32`

Notable row movement:

- `llvm-x86`
  - before: `selection_compile_only`
  - after: `disasm_ready`
- `llvm-riscv-lp64d`
  - before: `loader_unsupported_machine`
  - after: `disasm_runtime_failure`
- `llvm-mips64le`
  - before: `loader_unsupported_machine`
  - after: `disasm_runtime_failure`
- `llvm-ppc-64-le`
  - before: `loader_unsupported_machine`
  - after: `disasm_runtime_failure`
- `llvm-sparcv9-64`
  - before: `loader_unsupported_machine`
  - after: `disasm_runtime_failure`

### Interpretation

This wave did not solve broad runtime execution coverage. It did make the next
owner layers explicit:

- loader machine support is no longer the dominant blocker in the current LLVM
  smoke corpus
- compile-only runtime selection remains for 19 rows
- the remaining 32 rows now fail inside disasm/runtime execution rather than at
  loader admission

That is the intended ownership move for the next wave.

## 7. Semantic Emitter Cutover

This wave moved the first executable slice of `compiled_table.rs` away from the
handwritten mnemonic-family emitter and into compiled op-template execution.

### Scope

- compiler owner:
  - `crates/fission-sleigh/src/compiler/ir.rs`
- runtime owners:
  - `crates/fission-sleigh/src/runtime/spine/template.rs`
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`
  - `crates/fission-sleigh/examples/raw_pcode_probe.rs`

### What changed

- `RuntimeSleighFrontend` now exposes an additive `decode_and_lift_with_details(...)`
  API so the raw parity harness can observe whether an instruction actually used
  compatibility emission.
- `RuntimeTemplateEvaluator` no longer dispatches only through handwritten
  `emit_*` semantic-family methods.
  - if a constructor has executable `op_templates`, it now runs them first
  - if it has no executable template path yet, it still falls back to
    compatibility emission
- compatibility-lowered constructor templates now carry real operand-bearing
  `CompiledOpTpl` sequences for the currently supported primitive families:
  - `RETURN`
  - `CALL`
  - `BRANCH`
  - `COPY`
  - `INT_ZEXT`
  - `INT_SEXT`
  - `INT_ADD`
  - `INT_SUB`
  - `INT_AND`
  - `INT_OR`
  - `INT_XOR`
  - `INT_MULT`
  - `INT_LEFT`
  - `INT_RIGHT`
  - `INT_SRIGHT`
- handwritten semantic fallback remains the canonical owner for:
  - `jcc`
  - `cmp/test`
  - `setcc`
  - stack helpers
  - `lea`
  - accumulator extends

### Validation

- `cargo check -p fission-sleigh`
  - passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - passed
- `cargo check -p fission-cli`
  - passed
- `cargo build -p fission-cli --release`
  - passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - rerun deterministic
  - manifest remained `38 processors / 146 variants`

### Raw P-code gate outcome

Full feature-expanded manifest:

- report:
  - `benchmark/artifacts/raw_p_code_benchmark/20260424_cutover_full/aggregate_raw_pcode_parity_report.json`
- bucket totals:
  - `compat_emitter_used = 26`
  - `full_match = 4`
  - `pcode_op_count_mismatch = 23`
  - `pcode_opcode_mismatch = 36`
  - `varnode_space_mismatch = 2`
  - `unsupported_template = 1`

Compared with the previous expanded baseline:

- previous `compat_emitter_used = 48`
- current `compat_emitter_used = 26`

Feature slices now separate cleanly by owner:

- `control_flow`
  - `compat_emitter_used = 2`
  - report:
    - `benchmark/artifacts/raw_p_code_benchmark/20260424_cutover_control_flow/aggregate_raw_pcode_parity_report.json`
- `memory`
  - `compat_emitter_used = 0`
  - report:
    - `benchmark/artifacts/raw_p_code_benchmark/20260424_cutover_memory/aggregate_raw_pcode_parity_report.json`
- `stack`
  - `compat_emitter_used = 4`
  - report:
    - `benchmark/artifacts/raw_p_code_benchmark/20260424_cutover_stack/aggregate_raw_pcode_parity_report.json`
- `flags`
  - `compat_emitter_used = 1`
  - report:
    - `benchmark/artifacts/raw_p_code_benchmark/20260424_cutover_flags/aggregate_raw_pcode_parity_report.json`

### Interpretation

This wave did not solve raw parity end to end.

What it did prove:

- call/jump/return and simple move/extend families can execute through compiled
  op templates inside the common spine
- the parity harness now measures real compatibility-emitter usage instead of
  hardcoding it to `true`
- `control_flow` and `memory` are no longer completely blocked on the handwritten
  semantic emitter

What still directly owns the remaining mismatch:

- flag production and condition-code materialization
- stack helper lowering
- `lea` / address shaping
- unsupported template classification still buckets as a generic Fission runtime
  error no longer blocks owner attribution in the parity comparator

## 7. Compiled-Table Compatibility Holdout Narrowing

### Scope

- canonical owner:
  - `crates/fission-sleigh/src/compiler/ir.rs`
  - `crates/fission-sleigh/src/runtime/spine/decision.rs`
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`

### What changed

- constructor selection no longer bootstraps from `decision_root_keys(...)`.
  - the canonical path now starts from the generated global decision-tree root and
    closes at deterministic terminal constructor verification
- runtime leaf selection no longer drops non-`runtime_ready` constructor matches on
  the floor.
  - a matching unsupported constructor is now preserved as typed fallback instead of
    collapsing into `DecodeNoMatch`
- `CompiledConstructTplKind::Unsupported` was added as an explicit compatibility
  holdout marker in the compiler IR.
  - unsupported constructors remain visible in generated executable inventory
  - they still fail closed at lift time through `UnsupportedPcodeTemplate`
- token-field probe evaluation now tolerates constructors that do not materialize a
  ModRM/token bundle.
  - this removed the `ret` regression introduced by the root-bucket removal
- generated runtime tests were added for:
  - `fninit` decode without `DecodeNoMatch`
  - `fninit` typed `UnsupportedPcodeTemplate`

### Result

The remaining `_fpreset` first-instruction hole is no longer an unexplained decode
miss:

- before this wave:
  - `_fpreset @ 0x1400025c0` -> `DecodeNoMatch`
- after this wave:
  - `_fpreset @ 0x1400025c0` -> `UnsupportedPcodeTemplate: x86-64: unsupported_template_kind`

This is a strict fail-closed improvement. The common spine now distinguishes
"constructor matched but template is not executable yet" from "no constructor
matched at all."

## 8. Raw P-code Gate Readout After Holdout Narrowing

Canonical manifest run:

- command:
  - `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --output-dir benchmark/artifacts/raw_p_code_benchmark/20260424_compiled_table_holdout`

Bucket totals:

- `compat_emitter_used = 32`
- `fission_decode_error = 1`
- `pcode_opcode_mismatch = 22`
- `pcode_op_count_mismatch = 14`
- `full_match = 4`

Per-row interpretation:

- `test_functions.exe:_fpreset @ 0x1400025c0`
  - now fails closed as typed unsupported, not `DecodeNoMatch`
- `test_functions.exe:add @ 0x140001450`
  - still mismatches at raw P-code opcode/count level
- `test_functions.exe:fibonacci @ 0x140001470`
  - still mismatches at opcode ordering/selection level
- `test_functions.exe:WinMainCRTStartup @ 0x1400013e0`
  - still mismatches in startup flag semantics
- `test_functions.exe:mainCRTStartup @ 0x140001400`
  - still mismatches in startup flag semantics

### Interpretation

This wave fixed the constructor-selection failure family but did not yet improve raw
P-code parity totals. The next direct owner remains the compatibility semantic
emitter inside `compiled_table.rs`.

What improved:

- unexplained `_fpreset` decode hole removed
- unsupported constructors now surface as typed unsupported
- `ret` and other no-ModRM constructors remain decodable after removing
  root-bucket bootstrap

What still remains:

- `compat_emitter_used` is still present across the canonical rows
- opcode/count mismatch totals did not decrease
- startup/control-flow rows still diverge from Ghidra raw P-code because the
  mnemonic-family emitter remains the active semantic owner

## 9. Raw P-code Benchmark Feature Expansion

The raw P-code parity harness was expanded from a coarse 5-row gate into a
feature-tagged manifest.

Added benchmark capabilities:

- manifest rows now carry:
  - `feature_group`
  - `feature`
  - `owner`
  - `notes`
- `run_raw_pcode_parity.py` now supports:
  - `--feature`
  - `--group`
- aggregate reports now emit:
  - `feature_totals`
  - `group_totals`
- comparator buckets now include more explicit varnode/target classes:
  - `input_varnode_mismatch`
  - `output_varnode_mismatch`
  - `temp_space_mismatch`
  - `label_target_mismatch`

Expanded canonical manifest:

- legacy coarse rows were kept for continuity
- feature-isolated rows were added for:
  - `stack_prologue_sub`
  - `rip_relative_load`
  - `memory_store_imm`
  - `relative_call`
  - `relative_jump`
  - `return`
  - `lea`
  - `compare`
  - `push_prologue`
  - `cmp_and_jcc`
  - `unsupported_template`

Validation:

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
  - passed
- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --feature return --output-dir benchmark/artifacts/raw_p_code_benchmark/feature-return-check`
  - passed
- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --output-dir benchmark/artifacts/raw_p_code_benchmark/20260424_feature_expanded`
  - passed

Expanded aggregate readout:

- `row_count = 17`
- `feature_count = 14`
- `group_count = 7`

This does not claim semantic parity. It changes the benchmark surface so the next
runtime work can be sliced by owner family instead of by one mixed startup row.
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

## 7. Raw P-code Canonical Gate and ConstructTpl Compatibility Telemetry

This wave promoted the raw P-code benchmark from single-row smoke to a canonical
gate and added explicit compatibility telemetry to the current compiled-table
runtime path.

### Added

- `benchmark/raw_p_code_benchmark/canonical_rows.json`
- manifest-driven aggregate execution in `run_raw_pcode_parity.py`
- `compat_emitter_used` telemetry in the Fission raw P-code probe and compare
  report
- additive compiler IR scaffold for `ConstructTpl`-style primitive op templates
  with `template_source`

### Validation

- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --output-dir benchmark/artifacts/raw_p_code_benchmark/canonical-current`
  - result:
    - `compat_emitter_used = 30`
    - `decode_no_match = 1`
    - `pcode_opcode_mismatch = 22`
    - `pcode_op_count_mismatch = 14`
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `42 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `target/release/fission_cli decomp ... --addr 0x140001470 --json`
  - result: `rust_sleigh`, `fell_back=false`

### Interpretation

- the raw P-code gate is now stable enough to serve as the first correctness bar
  before full decompiler benchmark runs
- current execution is still visibly `compatibility_lowered`
- the next direct owner remains `compiled_table.rs` constructor/template
  execution, especially `_fpreset` and startup flag semantics

## 10. Flag / Branch / Addressing Semantic Holdout Cutover

This wave moved another set of handwritten semantic families into the common
`CompiledOpTpl` execution path.

### Scope

- canonical owner:
  - `crates/fission-sleigh/src/compiler/ir.rs`
  - `crates/fission-sleigh/src/compiler/codegen.rs`
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`
- reference owner:
  - `ConstructTpl`
  - `PcodeEmit`
  - `ParserWalker`
  - `ConstructState`
  - `SleighParserContext`

### What changed

- compiler-side executable IR now emits primitive op templates for:
  - `lea` / address-of constructors
  - stack store / stack load constructors
  - frame teardown
  - compare / test style flag materialization
  - accumulator-width sign extension
- new template-side varnode forms were added:
  - `effective_address`
  - persistent `temp{id,size}`
  - `fixed_register`
  - `flag`
- runtime generic template execution in `compiled_table.rs` now executes:
  - `LOAD`
  - `STORE`
  - `PIECE`
  - `SUBPIECE`
  - `INT_CARRY`
  - `INT_SCARRY`
  - `INT_SBORROW`
  - `POPCOUNT`
- generic template writes now support persistent temp reuse plus fixed-register
  and flag targets without dropping back to handwritten emission
- targeted runtime regression tests were added for:
  - compare template cutover
  - push template cutover
  - `lea` template cutover

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `47 passed / 0 failed`
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed

### Raw P-code Gate

Canonical aggregate:

- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --output-dir benchmark/artifacts/raw_p_code_benchmark/20260424_holdout_cutover`
  - result:
    - `compat_emitter_used = 8`
    - `pcode_opcode_mismatch = 24`
    - `pcode_op_count_mismatch = 24`
    - `unsupported_template = 1`

Delta against the previous full aggregate:

- `compat_emitter_used`
  - before: `26`
  - after: `8`
- `pcode_opcode_mismatch`
  - before: `36`
  - after: `24`
- `pcode_op_count_mismatch`
  - before: `23`
  - after: `24`

Group readout from the new aggregate:

- `addressing`
  - `pcode_opcode_mismatch = 2`
  - `pcode_op_count_mismatch = 2`
- `flags`
  - `pcode_opcode_mismatch = 1`
  - `pcode_op_count_mismatch = 1`
  - `temp_space_mismatch = 1`
  - `varnode_space_mismatch = 1`
- `stack`
  - `pcode_opcode_mismatch = 1`
  - `pcode_op_count_mismatch = 1`
  - `input_varnode_mismatch = 4`
  - `varnode_space_mismatch = 4`
- `control_flow`
  - `compat_emitter_used = 1`
  - `pcode_opcode_mismatch = 4`
  - `pcode_op_count_mismatch = 4`
  - `varnode_space_mismatch = 3`
- `unsupported`
  - `_fpreset` remains typed `unsupported_template`
  - no regression back to `DecodeNoMatch`

### Interpretation

This wave succeeded at moving compare / stack / addressing constructors out of
the handwritten emitter path. The canonical signal is the `compat_emitter_used`
drop from `26` to `8`.

What improved:

- compare / test style rows now execute through `CompiledOpTpl`
- push / stack-shaping rows no longer rely on the handwritten stack helper
- `lea` rows now go through template evaluation rather than the handwritten
  address helper
- `_fpreset` stays fail-closed as typed unsupported

What is now exposed more clearly:

- the next remaining control-flow owner is `emit_jcc`
- stack rows now show real operand/space mismatches instead of being hidden
  behind compatibility emission
- compare / branch rows still diverge on predicate varnode identity and temp
  space usage
- startup rows still contain control-flow and startup-sequence ordering drift

### Next Owner

The next direct owner remains `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`.

Priority order:

1. cut over `emit_jcc` and remove the remaining control-flow compatibility path
2. replace synthetic flag/predicate varnode shaping with template-owned identity
3. close stack/input varnode mismatches now that stack helpers are no longer the
   primary owner

## 11. Raw P-code Benchmark Ghidra Pinning

The raw p-code benchmark previously depended on whichever Ghidra installation
`pyghidra` discovered at runtime. That made the oracle version ambiguous even
though the current clean-room migration target is Ghidra 12.0.4.

### What changed

- `benchmark/raw_p_code_benchmark/ghidra_raw_pcode.py` now resolves the Ghidra
  install directory explicitly in this order:
  1. `--ghidra-dir`
  2. `GHIDRA_INSTALL_DIR`
  3. repo defaults:
     - `vendor/ghidra/ghidra-Ghidra_12.0.4_build`
     - `ghidra-Ghidra_12.0.4_build`
- the script now starts `pyghidra` with `install_dir=...`
- the emitted oracle JSON now records the resolved `ghidra_dir`
- `run_raw_pcode_parity.py` now forwards `--ghidra-dir` to the oracle script
- raw benchmark README examples were updated to show the explicit 12.0.4 pin

### Validation

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
  - result: passed
- `python3 benchmark/raw_p_code_benchmark/ghidra_raw_pcode.py --help`
  - result: shows `--ghidra-dir`
- `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --help`
  - result: shows `--ghidra-dir`

## 12. Architecture Readiness Truthful Bucketing + ARM/AARCH64 Promotion

The architecture readiness lane was previously mixing two different failure
owners:

- rows that were still compile-only in runtime registry selection
- rows that had actually entered the runtime path and then failed during decode

Because the `compile-only` message was split across wrapped stderr lines, the
runner misbucketed many rows as `disasm_runtime_failure`. That made the next
owner ambiguous.

### What changed

- `benchmark/full_benchmark/run_architecture_readiness_parallel.py`
  - added whitespace-normalized error classification
  - added row-level `normalized_disasm_error`
  - added row-level `classification_reason`
  - `compile-only entry` detection now survives wrapped/newline formatted stderr
- `crates/fission-sleigh/src/compiler/mod.rs`
  - replaced the x86-only runtime-status special case with a small explicit
    executable allowlist
  - current allowlist:
    - `x86`
    - `x86-64`
    - `AARCH64`
    - `AARCH64BE`
    - `AARCH64_AppleSilicon`
    - `ARM7_le`
    - `ARM7_be`
- `crates/fission-sleigh/src/runtime/registry.rs`
  - added tests that pin `AARCH64*` and `ARM7*` selection as
    `ExecutableCandidate`
  - kept `RISCV` pinned as compile-only in this wave

### Validation

- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo test -p fission-sleigh resolves_arm_and_aarch64_variants_as_executable_candidates -- --nocapture`
  - result: passed
- `cargo check -p fission-cli`
  - result: passed
- `cargo check -p fission-core`
  - result: passed
- architecture readiness rerun:
  - output:
    `benchmark/artifacts/full_benchmark/architecture_readiness_next_wave/architecture_readiness_aggregate.json`

### Benchmark result

Compared to
`benchmark/artifacts/full_benchmark/architecture_readiness_llvm53_after_release/architecture_readiness_aggregate.json`:

- `selection_compile_only: 19 -> 12`
- `disasm_runtime_failure: 32 -> 39`
- `disasm_ready: 2 -> 2`
- `inventory_ready: 51 -> 51`

The important change is not the top-level readiness class. The important change
is that the targeted ARM/AARCH64 rows now leave `selection_compile_only` and
enter the actual runtime decode path:

- `llvm-aarch64` -> `disasm_runtime_failure`
- `llvm-aarch64be` -> `disasm_runtime_failure`
- `llvm-aarch64-applesilicon` -> `disasm_runtime_failure`
- `llvm-arm7-le` -> `disasm_runtime_failure`
- `llvm-arm7-be` -> `disasm_runtime_failure`

Current normalized errors for those rows are real runtime failures:

- `DecodeNoMatch: AARCH64 has no match at 0x0`
- `DecodeNoMatch: AARCH64BE has no match at 0x0`
- `DecodeNoMatch: AARCH64_AppleSilicon has no match at 0xa0`
- `DecodeNoMatch: ARM7_le has no match at 0x0`
- `DecodeNoMatch: ARM7_be has no match at 0x0`

This means the next owner is now correctly exposed as the runtime decode spine,
not manifest selection.

## 13. Raw P-code Template Parity Cutover

This wave moved the remaining control-flow compatibility holdout for conditional
branches into the compiled op-template path, then corrected `CALL` / `RET` raw
p-code shape so stack side effects are no longer hidden behind one-op shorthand.

### Scope

- compiler owners:
  - `crates/fission-sleigh/src/compiler/ir.rs`
  - `crates/fission-sleigh/src/compiler/codegen.rs`
- runtime owners:
  - `crates/fission-sleigh/src/runtime/spine/template.rs`
  - `crates/fission-sleigh/src/runtime/spine/compiled_table.rs`
  - `crates/fission-sleigh/src/runtime/spine/emitter.rs`
- benchmark owners:
  - `benchmark/raw_p_code_benchmark/compare_raw_pcode.py`
  - `benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py`
  - `crates/fission-sleigh/examples/raw_pcode_probe.rs`

### What changed

- added `ConditionPredicate` as an explicit template varnode so `Jcc` / `Setcc`
  constructors can execute through `CompiledOpTpl` instead of using
  `emit_jcc` / `emit_setcc` as the primary path
- changed `CALL` templates from one raw `CALL` op to:
  - `INT_SUB` stack pointer update
  - `STORE` return address
  - `CALL` target
- changed `RET` templates from one raw `RETURN` op to:
  - `LOAD` return target from stack
  - `INT_ADD` stack pointer update
  - `RETURN` target
- added `template_source` to runtime execution telemetry and raw p-code probe
  output
- expanded row-level raw p-code mismatch reporting with:
  - Ghidra opcode sequence
  - Fission opcode sequence
  - first differing op index
  - normalized varnode payloads
  - template source
  - owner hints
- added aggregate `owner_hint_totals`

### Raw P-code Gate

Baseline report:

- `benchmark/artifacts/raw_p_code_benchmark/20260424_canonical_speed/aggregate_raw_pcode_parity_report.json`

New report:

- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_full/aggregate_raw_pcode_parity_report.json`

Bucket deltas:

- `compat_emitter_used: 8 -> 7`
- `pcode_opcode_mismatch: 24 -> 16`
- `pcode_op_count_mismatch: 24 -> 16`
- `unsupported_template: 1 -> 1`
- `full_match: 4 -> 4`
- `input_varnode_mismatch: 12 -> 20`
- `varnode_space_mismatch: 16 -> 24`

Group deltas:

- `control_flow`
  - `compat_emitter_used: 1 -> 0`
  - `pcode_opcode_mismatch: 4 -> 1`
  - `pcode_op_count_mismatch: 4 -> 1`
- `compound`
  - `pcode_opcode_mismatch: 14 -> 9`
  - `pcode_op_count_mismatch: 14 -> 9`
- `flags`, `stack`, `memory`, `addressing`
  - opcode/count mismatch did not regress

The varnode mismatch increase is expected for this wave: `CALL` / `RET` now emit
Ghidra-shaped stack side-effect opcodes, which exposes the next space/register
identity owner instead of hiding it behind one-op shorthand.

Feature slice reports:

- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_control_flow/aggregate_raw_pcode_parity_report.json`
- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_flags/aggregate_raw_pcode_parity_report.json`
- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_stack/aggregate_raw_pcode_parity_report.json`
- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_addressing/aggregate_raw_pcode_parity_report.json`
- `benchmark/artifacts/raw_p_code_benchmark/template_cutover_unsupported/aggregate_raw_pcode_parity_report.json`

Note: the checked-in vendor tree is the Ghidra 12.0.4 clean-room reference source,
but the raw p-code runner requires a packaged launchable install root. The
benchmark execution used `/Users/sjkim1127/Downloads/ghidra_12.0.4_PUBLIC`, which
is the packaged 12.0.4 oracle.

### Architecture Readiness Guard

Report:

- `benchmark/artifacts/full_benchmark/architecture_readiness_after_template_cutover/architecture_readiness_aggregate.json`

Compared with
`benchmark/artifacts/full_benchmark/architecture_readiness_next_wave/architecture_readiness_aggregate.json`:

- `disasm_ready: 2 -> 2`
- `inventory_ready: 51 -> 51`
- `selection_compile_only: 12 -> 12`
- `disasm_runtime_failure: 39 -> 39`

ARM/AARCH64 rows stayed in the runtime decode path and did not regress back to
`selection_compile_only`.

### Validation

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `49 passed / 0 failed`
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed

### Remaining Owner

The next direct owner is no longer `emit_jcc`; it is varnode identity and
language-space mapping inside the shared runtime template evaluator:

1. replace synthetic `const_u64(0, 8)` RAM-space placeholders with language
   metadata-derived space IDs
2. align stack/register varnode identity for `CALL`, `RET`, `PUSH`, and memory
   access templates
3. replace synthetic flag predicate temporaries with template-owned condition
   varnode identity
