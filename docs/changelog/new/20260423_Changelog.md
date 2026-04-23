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

## 3. Ghidra Common Runtime Spine Extraction

### Scope

- wave type:
  - runtime architecture / clean-room owner migration
- primary owner:
  - `SleighLanguage -> DecisionNode -> ConstructState/ParserWalker -> ConstructTpl -> PcodeEmit`
- goal:
  - stop treating x86-64 as the runtime design center
  - keep x86-64 as the first executable consumer of an ISA-independent SLEIGH spine

### What changed

- added a shared runtime spine under `crates/fission-sleigh/src/runtime/spine/`.
  - `context.rs` owns the parser-context-level instruction facts
  - `decision.rs` owns generic decision-tree traversal and match traces
  - `construct.rs` owns constructor state, handles, and bound operand shape
  - `walker.rs` owns parser-walker construct node recording
  - `template.rs` owns semantic template dispatch
  - `emitter.rs` owns p-code sequence, unique temporary allocation, and op emission
- `generated_x86_64.rs` now consumes the common spine.
  - x86 keeps prefix/REX/ModRM/SIB extraction and x86 register/flag mapping
  - decision traversal, construct state, template dispatch, and p-code emitter are no longer x86-only owners
- README/AGENTS now document the fixed Ghidra clean-room owner mapping.

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `37 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
  - generated artifact diff: empty

Limited same-axis benchmark:

- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-sleigh-decisiontree-parserwalker-v2-latest`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-ghidra-spine-latest`
- weighted average normalized similarity:
  - `44.56% -> 44.56%`
- x64 split:
  - `6` binaries, failed: `none`
- release promotion:
  - still `false`

### Result

This wave is behavior-preserving architecture work. It does not claim new x86-64 parity by itself.
The important improvement is that the next executable consumer, especially `AARCH64`, can reuse the
same runtime spine instead of copying the x86 generated runtime shape.

Deletion gate:

- `iced-x86` remains retained as oracle/bridge
- x86-64 generated runtime remains the first executable parity target

## 4. Runtime Processor Tree Layout

### Scope

- wave type:
  - runtime architecture / ownership layout
- primary owner:
  - `runtime/spine`
  - `runtime/processors`
  - `runtime/oracle`
- goal:
  - make architecture-specific folders explicit without moving semantics back out of the common spine

### What changed

- moved the generated x86-64 runtime consumer to:
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- moved the temporary `iced-x86` bridge to:
  - `crates/fission-sleigh/src/runtime/oracle/x86_64_iced.rs`
- added processor/oracle module roots:
  - `crates/fission-sleigh/src/runtime/processors/mod.rs`
  - `crates/fission-sleigh/src/runtime/processors/x86/mod.rs`
  - `crates/fission-sleigh/src/runtime/oracle/mod.rs`
- updated `RuntimeSleighFrontend` routing:
  - generated gate on: `processors::x86::generated`
  - generated gate off: `oracle::x86_64_iced`
- updated README/AGENTS runtime tree documentation.

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `37 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed

Limited same-axis benchmark:

- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-ghidra-spine-latest`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-processor-tree-latest`
- weighted average normalized similarity:
  - `44.560% -> 44.560%`
- x64 split:
  - `6` binaries, failed: `none`
- release promotion:
  - still `false`

### Result

This wave is behavior-preserving. It makes the next architecture additions explicit:

- `runtime/spine` remains the shared Ghidra-style runtime owner
- `runtime/processors/<processor>/` owns processor adapters only
- `runtime/oracle/` contains temporary differential bridges only

The next executable owner remains x86-64 parity under the common spine. After that, `AARCH64`
should be added as the second processor adapter without copying x86 semantic ownership.

## 5. Full Processor Runtime Skeleton Tree

### Scope

- wave type:
  - runtime architecture / skeleton registration
- primary owner:
  - `crates/fission-sleigh/src/runtime/processors/`
- goal:
  - mirror all Ghidra Processor families at the runtime adapter boundary
  - keep executable semantics centralized in `runtime/spine`

### What changed

- added checked-in processor adapter skeleton folders for all `38` mirrored Ghidra processors.
- retained Rust-safe module naming for non-identifier Processor names:
  - `6502` -> `p_6502`
  - `68000` -> `p_68000`
  - `8048` -> `p_8048`
  - `8051` -> `p_8051`
  - `8085` -> `p_8085`
  - `PA-RISC` -> `pa_risc`
- added `ProcessorSkeleton` metadata and `PROCESSOR_SKELETONS`.
- added a test that compares processor skeleton coverage against the runtime registry manifest.
- `x86` remains the only executable candidate.
- all other processor folders are compile-only skeletons and must fail closed until parity gates are implemented.

### Validation

- `cargo fmt`
- `cargo check -p fission-sleigh`
- `cargo test -p fission-sleigh -- --test-threads=1`
  - expected owner check:
    - `processor_skeletons_cover_all_ghidra_processors`
- `cargo check -p fission-cli`

### Result

This wave does not broaden executable runtime support. It creates the stable folder boundary needed
for processor-by-processor promotion:

1. shared runtime semantics remain in `runtime/spine`
2. processor folders hold adapter metadata and future field/register/address-space extraction
3. temporary bridge/oracle code stays isolated under `runtime/oracle`

Next executable promotion order remains:

1. `x86`
2. `AARCH64`
3. `ARM`
4. `MIPS`
5. `PowerPC`
6. `RISCV`
7. long-tail processors

## 6. Full Processor Runtime Foundation Contract

### Scope

- wave type:
  - runtime foundation / compile-only fail-closed contract
- primary owner:
  - `crates/fission-sleigh/src/runtime/spine/language.rs`
  - `crates/fission-sleigh/src/compiler/equivalence.rs`
- goal:
  - turn the full Ghidra processor skeleton tree into an observable runtime promotion contract
  - keep unsupported processors fail-closed while still proving their checked-in specs compile into a runtime attempt report

### What changed

- added `LanguageRuntime` as the common compiled-table runtime working state.
- added `ProcessorRuntimeProfile` with processor, module, entry, endian, addressable-unit, unique-space, and runtime status metadata.
- added `RuntimeAttemptReport` as the canonical readout for whether a registered processor is compile-only or executable-candidate.
- added `RuntimeEndian` and conservative manifest/name based endian inference for reporting.
- exposed `RuntimeSleighFrontend::compile_language_runtime()` and `RuntimeSleighFrontend::runtime_attempt_report()`.
- added generic runtime fixture parity report types:
  - `RuntimeParityFixture`
  - `RuntimeParityVarnodeShape`
  - `RuntimeParityReport`
  - `RuntimeParityRecord`
- re-exported the runtime parity report API from `fission_sleigh::compiler` so it is a real downstream contract, not test-only dead code.
- compile-only processors still return `UnsupportedGeneratedSemantic` on decode/lift; no fake p-code emission was added.

### Validation

- `cargo fmt -p fission-sleigh`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `40 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: `38 processors / 146 variants`
- generated artifact determinism:
  - reran `generate_sleigh_frontends`
  - `git diff --quiet -- crates/fission-sleigh/generated`
  - result: empty diff

Limited generated-runtime benchmark:

- env:
  - `FISSION_ENABLE_GENERATED_X86_64_RUNTIME=1`
- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-processor-tree-latest`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-full-processor-runtime-foundation-latest`
- compact summary:
  - `benchmark/artifacts/full_benchmark/windows-small-c-full-processor-runtime-foundation-latest/benchmark_compact_summary.json`
- limit:
  - first `5` canonical seed functions per binary
- weighted average normalized similarity:
  - `44.560%`
- x64 split:
  - `6` binaries
- promotion blockers:
  - `advisory_gate_mode`
  - row-fidelity gate failed for `math-operations`, `test-functions`
- result:
  - no runtime crash regression observed in the limited x86 generated-runtime lane
  - release promotion remains blocked, as expected for this foundation-only wave

### Result

This wave does not promote the remaining `37` processors to executable runtime. It makes processor promotion auditable:

1. every processor can be looked up through the manifest-backed registry skeleton
2. compile-only processors can produce a typed runtime attempt report
3. decode/lift remains fail-closed until a processor reaches an executable parity gate
4. fixture parity reports now have a reusable, processor-independent schema

Next owner remains x86-64 runtime parity under the common Ghidra-style spine:

1. `DecisionNode` constructor selection parity
2. `ConstructState` / `ParserWalker` operand and handle parity
3. `ConstructTpl` / `PcodeEmit` template evaluator parity

## 7. x86-64 Oracle Hard Delete

### Scope

- wave type:
  - runtime routing / deletion
- primary owner:
  - `crates/fission-sleigh/src/runtime/mod.rs`
  - `crates/fission-sleigh/src/compiler/equivalence.rs`
- goal:
  - remove the temporary `iced-x86` oracle/bridge from `fission-sleigh`
  - make generated x86-64 runtime the only executable decode owner in this crate

### What changed

- deleted:
  - `crates/fission-sleigh/src/runtime/oracle/mod.rs`
  - `crates/fission-sleigh/src/runtime/oracle/x86_64_iced.rs`
- removed the `iced-x86` dependency from `fission-sleigh`.
- removed the x86-64 env gate from runtime routing:
  - `FISSION_ENABLE_GENERATED_X86_64_RUNTIME`
- `RuntimeSleighFrontend::decode_and_lift_with_len()` now routes x86-64 executable decode directly to:
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- removed bridge-based equivalence API and tests that depended on the deleted oracle path.
- retained processor-independent parity fixture reporting:
  - `RuntimeParityFixture`
  - `RuntimeParityReport`
  - `RuntimeParityRecord`
  - `RuntimeParityVarnodeShape`

### Validation

- `cargo fmt -p fission-sleigh`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: passed
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- limited Windows small C benchmark:
  - trial artifact:
    - `benchmark/artifacts/full_benchmark/windows-small-c-full-processor-runtime-foundation-latest`
  - weighted average normalized similarity:
    - `44.560%`
  - result:
    - non-worse against the current generated-runtime baseline in the limited lane

### Result

`fission-sleigh` no longer carries a temporary x86 decode bridge. The crate now has one executable x86-64 runtime owner and one processor-independent compile-only fail-closed path for the remaining processors.

The next owner is not “remove more old code”; it is still x86-64 semantic parity inside the generated runtime:

1. constructor selection precision
2. operand/handle parity
3. template evaluator parity

## 8. `fission-disasm` Crate Removal and Runtime Disasm Surface

### Scope

- wave type:
  - dependency deletion / runtime owner migration
- primary owner:
  - `fission-sleigh::runtime`
- goal:
  - remove the standalone `fission-disasm` crate
  - remove direct `iced-x86` use from the CLI disasm path
  - keep x86-64 disasm output routed through the generated SLEIGH runtime owner

### What changed

- added runtime-owned disasm payloads:
  - `DecodedInstruction`
  - `DecodedFlowKind`
- added runtime decode helpers:
  - `RuntimeSleighFrontend::decode_window`
  - `RuntimeSleighFrontend::discover_direct_call_targets`
- moved CLI disasm rendering to the runtime decode surface:
  - `disasm`
  - `disasm --function`
  - decomp preview assembly snippets
- removed standalone crate wiring:
  - removed `crates/fission-disasm` from the workspace
  - removed `fission-disasm` dependencies from `fission-loader` and `fission-pcode`
  - removed `fission-pcode::disasm`
  - removed `fission-static::analysis::disasm`
- removed direct `iced-x86` dependency from `fission-cli`.
- removed unused direct `iced-x86` dependency from `fission-pcode`.
- loader function discovery no longer depends on `fission-disasm`.
  - Direct call target harvesting is now a cycle-free internal scan in `fission-loader`.
  - A direct `fission-loader -> fission-sleigh` dependency was not introduced because the current graph would cycle through `fission-sleigh -> fission-pcode -> fission-loader`.

### Validation

- `cargo fmt -p fission-sleigh -p fission-loader -p fission-cli -p fission-pcode -p fission-static`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-loader`
  - result: passed
- `cargo check -p fission-pcode`
  - result: passed
- `cargo check -p fission-static`
  - result: passed
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- CLI smoke:
  - `target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --count 8`
  - result: generated-runtime disassembly emitted x86-64 instructions
- CLI function smoke:
  - `target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --function`
  - result: function disassembly output emitted through the runtime decode surface
- decomp smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: JSON output produced successfully

### Result

`fission-disasm` is no longer a workspace crate, and the CLI disasm surface no longer owns an `iced-x86` decode path. The remaining direct `iced-x86` users are separate analysis/UI/signature surfaces outside this crate-removal wave:

- `fission-static` xref analysis
- `fission-signatures` FID hashing
- `fission-tauri` UI analysis/listing helpers
- `fission-dynamic`

The next owner is to migrate those remaining analysis surfaces to the same generated runtime decode API or to explicitly keep them as independent non-SLEIGH utilities with documented ownership.
