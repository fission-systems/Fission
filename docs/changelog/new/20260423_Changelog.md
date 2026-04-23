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

## 9. Repo-wide `iced-x86` Removal and Runtime Decode Owner Consolidation

### Scope

- wave type:
  - dependency deletion / runtime decode owner migration
- primary owner:
  - `fission-sleigh::runtime`
- goal:
  - remove all remaining repo-wide `iced-x86` references
  - move xref/listing/assembly/CFG/string-xref decode consumers to `RuntimeSleighFrontend`
  - remove cycle-prone FID hashing until a SLEIGH-backed owner can be reintroduced without dependency cycles

### What changed

- extended runtime disasm payloads:
  - added `DecodedReference`
  - added `DecodedReferenceKind`
  - added `DecodedInstruction::references`
- migrated static xref analysis:
  - `fission-static::analysis::xrefs` now consumes `RuntimeSleighFrontend::decode_window`
  - call/jump/data xrefs are derived from `DecodedFlowKind` and `DecodedReferenceKind`
- migrated Tauri decode consumers:
  - assembly view
  - listing view
  - CFG builder
  - xrefs command
  - string xrefs
  - cross-image wrapper detection
- added a Tauri runtime decode helper module:
  - `crates/fission-tauri/src-tauri/src/services/runtime_decode.rs`
- removed remaining direct `iced-x86` dependencies from:
  - `fission-static`
  - `fission-signatures`
  - `fission-dynamic`
  - `fission-tauri`
- removed `fission-signatures::fid_hash`.
  - Reason: `fission-signatures -> fission-sleigh` would create a cycle through `fission-sleigh -> fission-pcode -> fission-signatures`.
  - Reintroduction owner: cycle-free SLEIGH-backed FID hashing, likely under `fission-sleigh` or a lower-level shared signature utility crate.
- updated the desktop About dialog from `iced-x86` to `Fission SLEIGH runtime`.

### Validation

- `rg -n "iced-x86|iced_x86|use iced_x86|fission_disasm|fission-disasm" Cargo.toml Cargo.lock crates -g '!**/target/**'`
  - result: `0` matches
- `rg -n "compute_fid_hash|FidHashQuad|FnvHasher64|fid_hash" crates Cargo.toml`
  - result: `0` matches
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-static`
  - result: passed
- `cargo test -p fission-static xref -- --test-threads=1`
  - result: `3 passed / 0 failed`
- `cargo check -p fission-signatures`
  - result: passed
- `cargo check -p fission-dynamic`
  - result: passed
- `cargo check -p fission-cli`
  - result: passed
- `cargo check -p fission-tauri`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- CLI smoke:
  - `target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --count 8`
  - result: generated-runtime disassembly emitted x86-64 instructions
- decomp smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: JSON output produced successfully

### Result

`iced-x86` is no longer present in `Cargo.toml`, `Cargo.lock`, or `crates/`. Decode/disasm/xref/CFG/listing surfaces now consume the same runtime decode owner instead of maintaining parallel decoder logic in static analysis or UI layers.

### Remaining risk / next owner

- 32-bit x86 and non-x86 executable decode remain typed fail-closed until their SLEIGH runtime consumers are promoted.
- FID hashing is intentionally absent until a cycle-free SLEIGH-backed implementation is added.
- The next owner is x86-64 generated-runtime semantic parity:
  1. data-reference precision for memory operands
  2. richer template execution for startup/control-heavy rows
  3. cycle-free FID hash reintroduction

## 10. Loader Heuristic Removal and Ghidra-Style Analyzer Split

### Scope

- wave type:
  - owner cleanup / behavior-preserving loader reduction
- primary owner:
  - `fission-loader` for authoritative binary metadata
  - `fission-static::analysis::function_discovery` for SLEIGH-runtime function discovery
- goal:
  - remove x86/x64 byte-pattern function discovery from the loader
  - keep loader free of `fission-sleigh` / `fission-static` dependencies
  - route CLI and Tauri discovery requests through the analyzer layer

### What changed

- removed loader-owned function discovery heuristics:
  - rel32 `CALL` / `JMP` byte scans
  - x86/x64 prologue pattern tables
  - `discover_internal_functions*`
  - `discover_functions_by_prologue*`
- removed PE stripped fallback linear prologue sweep.
  - PE loader now preserves metadata-only behavior: entry point, exports/imports, COFF, PDB, pdata, DWARF/format metadata.
- added `fission-static::analysis::function_discovery`.
  - Uses `RuntimeSleighFrontend::decode_window`.
  - Collects direct call targets for conservative/balanced profiles.
  - Collects direct branch targets only for aggressive profile.
  - Accepts only targets inside executable sections and not already present in the loader function index.
  - Unsupported runtime returns a no-op report instead of fake function discovery.
- added analyzer report fields:
  - `decoded_instruction_count`
  - `call_target_count`
  - `jump_target_count`
  - `accepted_function_count`
  - `unsupported_runtime`
- migrated consumers:
  - `fission-cli` one-shot discovery profile handling now calls the static analyzer.
  - Tauri `open_file`, `analyze_functions`, and `deep_scan_functions` now call the static analyzer.
- stabilized the sample-dependent PDB sidecar unit test to skip when the optional `fauxware.exe` fixture is absent.

### Validation

- `cargo check -p fission-loader`
  - result: passed
- `cargo test -p fission-loader -- --test-threads=1`
  - result: `25 passed / 0 failed / 2 doctests ignored`
- `cargo check -p fission-static`
  - result: passed
- `cargo test -p fission-static function_discovery -- --test-threads=1`
  - result: `3 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo check -p fission-tauri`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- static audit:
  - `rg -n "discover_internal_functions|discover_functions_by_prologue|FunctionDiscoveryProfile" crates/fission-loader`
  - result: `0` matches
- static audit:
  - `rg -n "E8|E9|PROLOGUE|prologue|collect_rel_call_targets|collect_rel_jmp_targets|scan_prologue_functions" crates/fission-loader`
  - result: no function-discovery heuristic matches remain; residual `E8` hits are detector signature bytes under `crates/fission-loader/src/detector/signatures.rs`
- dependency audit:
  - `rg -n "fission-sleigh|fission_static" crates/fission-loader/Cargo.toml crates/fission-loader/src`
  - result: `0` matches
- CLI smoke:
  - `target/release/fission_cli list benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --json`
  - result: JSON list emitted, `function_count=118`
- CLI smoke:
  - `target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --count 8 --json`
  - result: `8` disassembly rows emitted
- CLI smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: JSON decompilation output emitted

### Result

`fission-loader` is now metadata-only for function discovery ownership. Byte-pattern and prologue-based function discovery no longer lives in the loader or PE parser. The new analyzer layer preserves the existing CLI/Tauri profile surface while making the implementation SLEIGH-runtime based and fail-closed for unsupported runtimes.

### Remaining risk / next owner

- x86-64 direct-control-flow discovery is now runtime-driven, but recall is bounded by current generated runtime decode parity.
- Non-x86 executable discovery remains no-op until those SLEIGH runtime consumers are promoted.
- The next owner is still generated-runtime semantic parity:
  1. x86-64 `DecisionNode` / `ParserWalker` parity
  2. richer `ConstructTpl` / `PcodeEmit` execution
  3. cycle-free SLEIGH-backed FID hashing reintroduction

## 11. Shared Runtime Dispatch Ownership Extraction

### Scope

- wave type:
  - owner cleanup / behavior-preserving shared runtime refactor
- primary owner:
  - `crates/fission-sleigh/src/runtime/processors/mod.rs`
  - `crates/fission-sleigh/src/runtime/spine/language.rs`
  - `crates/fission-sleigh/src/runtime/mod.rs`
- goal:
  - remove duplicated x86-only executable readiness and dispatch checks from `runtime/mod.rs`
  - make processor/profile ownership decide runtime readiness and execution routing
  - keep x86-64 as the first executable consumer without changing the public runtime API

### What changed

- added shared processor-owned runtime readiness:
  - `processors::status_for_entry(entry)`
  - readiness now lives under processor/runtime ownership instead of duplicated `runtime/mod.rs` and `spine/language.rs` checks
- added shared processor-owned execution routing:
  - `processors::decode_and_lift(...)`
  - `processors::decode_instruction(...)`
  - `RuntimeSleighFrontend` no longer directly knows about `processors::x86::generated`
- reduced x86 exposure to processor-adapter boundary:
  - `crates/fission-sleigh/src/runtime/processors/x86/mod.rs` now exposes:
    - `supports_entry`
    - `decode_and_lift`
    - `decode_instruction`
  - `generated.rs` remains the first executable consumer, but is now reached through the processor adapter instead of direct top-level hardcoding
- preserved fail-closed behavior:
  - executable status without a registered processor execution engine still returns typed `UnsupportedPcodeTemplate`
  - compile-only variants still return typed `UnsupportedGeneratedSemantic`

### Validation

- `cargo fmt -p fission-sleigh`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: passed
- `git diff -- crates/fission-sleigh/generated`
  - result: empty

### Result

The runtime no longer has two separate x86-only readiness owners. `RuntimeSleighFrontend` now consumes processor-owned readiness and routing, which is the correct direction for the Ghidra-style `Language -> Decision -> ParserWalker -> TemplateEvaluator` spine. This does not yet change semantic parity, but it removes one architectural blocker to making `x86-64` just the first consumer of a shared execution engine.

### Remaining risk / next owner

- The semantic owner is still not fully extracted from `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`.
- `CompiledDecisionProbe` and the template evaluator are still x86-shaped in places, so this wave is not enough for second-consumer promotion.
- The next owner remains:
  1. generic `DecisionMatcher` probe vocabulary beyond x86-specific probe kinds
  2. `RuntimeParserWalker` / `RuntimeConstructState` extraction from x86 generated runtime
  3. template evaluation ownership migration from x86 semantic emitter helpers into shared spine contracts

## 12. Runtime Processor Inventory Extraction

### Scope

- wave type:
  - owner cleanup / behavior-preserving inventory migration
- primary owner:
  - `crates/fission-sleigh/src/runtime/registry.rs`
  - `crates/fission-sleigh/src/runtime/processors/mod.rs`
  - `crates/fission-sleigh/src/runtime/spine/language.rs`
- goal:
  - remove non-x86 processor skeleton inventory from `runtime/processors/*`
  - replace `PROCESSOR_SKELETONS` with a manifest-driven runtime registry
  - keep only the first executable x86-64 consumer path under `runtime/processors/`

### What changed

- added manifest-driven runtime registry:
  - `ProcessorDescriptor`
  - `RuntimeVariantDescriptor`
  - `RuntimeSupportLevel`
  - `ExecutionProviderKey`
  - `CompiledRuntimeRegistry`
- registry source of truth is now the checked-in language manifest:
  - `crates/fission-sleigh/specs/ghidra_language_manifest.json`
- `ProcessorRuntimeProfile::from_entry()` no longer reads processor skeleton modules.
  - It now resolves processor/module/endian/support from `runtime::registry`.
- `RuntimeSleighFrontend` and runtime discovery now use registry-owned runtime status.
- removed non-x86 processor skeleton source files from `runtime/processors/`.
  - remaining files under `runtime/processors/` are now only:
    - `mod.rs`
    - `x86/mod.rs`
    - `x86/generated.rs`
- preserved fail-closed behavior:
  - compile-only variants still return typed unsupported runtime errors
  - x86-64 remains the only executable candidate path

### Validation

- `cargo fmt -p fission-sleigh`
  - result: passed
- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: passed
- `git diff -- crates/fission-sleigh/generated`
  - result: empty
- CLI smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: passed
- inventory audit:
  - `find crates/fission-sleigh/src/runtime/processors -type f | sort`
  - result:
    - `crates/fission-sleigh/src/runtime/processors/mod.rs`
    - `crates/fission-sleigh/src/runtime/processors/x86/mod.rs`
    - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- static audit:
  - `rg -n "PROCESSOR_SKELETONS" crates/fission-sleigh/src`
  - result: `0` matches

### Result

The non-x86 processor inventory is no longer represented as per-processor Rust skeleton files. Runtime processor/variant ownership now comes from the checked-in manifest through `runtime/registry.rs`. This makes the remaining `runtime/processors/` subtree an execution-only holdout for the first x86-64 consumer, which is the correct starting point for the next deletion wave.

### Remaining risk / next owner

- `runtime/processors/` still exists because x86-64 execution is still routed through:
  - `runtime/processors/x86/mod.rs`
  - `runtime/processors/x86/generated.rs`
- provider extraction is still pending:
  1. move x86 execution entrypoint to `runtime/providers/x86_64.rs`
  2. delete `runtime/processors/x86/mod.rs`
  3. then extract semantic owner out of `x86/generated.rs`

## 13. Runtime Provider Extraction and X86 Field Helper Split

### Scope

- wave type:
  - owner cleanup / behavior-preserving runtime routing migration
- primary owner:
  - `crates/fission-sleigh/src/runtime/providers/`
  - `crates/fission-sleigh/src/runtime/quirks/x86_fields.rs`
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- goal:
  - remove `runtime/processors/x86/mod.rs` from the executable path
  - route x86-64 execution through a provider layer
  - split x86 field extraction and disassembly helpers out of `generated.rs`

### What changed

- added provider dispatch owner:
  - `crates/fission-sleigh/src/runtime/providers/mod.rs`
  - `crates/fission-sleigh/src/runtime/providers/x86_64.rs`
- `RuntimeSleighFrontend::{decode_and_lift_with_len, decode_instruction_with_len}` now dispatch through:
  - `registry::executable_provider_key_for_entry(...)`
  - `providers::provider_for_key(...)`
  - provider-owned `decode_*` entrypoints
- deleted `crates/fission-sleigh/src/runtime/processors/x86/mod.rs`
- added x86 helper boundary:
  - `crates/fission-sleigh/src/runtime/quirks/mod.rs`
  - `crates/fission-sleigh/src/runtime/quirks/x86_fields.rs`
- moved x86-specific helper ownership out of `generated.rs`:
  - instruction context / prefix parse
  - ModRM/SIB/displacement extraction
  - candidate bucket derivation
  - immediate signed/unsigned reads
  - register/disassembly formatting helpers
  - Jcc suffix formatting
- `crates/fission-sleigh/src/runtime/processors/x86/generated.rs` remains the first executable consumer, but it is now reached through the provider layer and consumes x86 helper code from `runtime/quirks/`
- `runtime/processors/` is now reduced to:
  - `crates/fission-sleigh/src/runtime/processors/mod.rs`
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`

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
  - result: passed
- `git diff -- crates/fission-sleigh/generated`
  - result: empty
- CLI smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x1400013e0 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001400 --json`
  - result: passed
- routing audit:
  - `rg -n "processors::x86::|runtime::processors|processors::decode_|processors::" crates/fission-sleigh/src/runtime`
  - result: `0` matches
- folder audit:
  - `find crates/fission-sleigh/src/runtime/processors -type f | sort`
  - result:
    - `crates/fission-sleigh/src/runtime/processors/mod.rs`
    - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`

### Result

The executable routing owner is no longer `runtime/processors`. X86-64 now enters the runtime through a provider layer, which matches the intended `registry -> provider -> shared spine` direction. The x86 helper boundary is also explicit now: prefix/field extraction and textual register formatting moved to `runtime/quirks/x86_fields.rs`, reducing the amount of non-semantic glue mixed into `generated.rs`.

### Remaining risk / next owner

- `crates/fission-sleigh/src/runtime/processors/x86/generated.rs` still owns too much semantic/runtime behavior:
  - constructor binding walk
  - template evaluation
  - p-code emission helpers
- `RuntimeTemplateEvaluator` and `CompiledDecisionProbe` are still x86-shaped in places
- next owner remains:
  1. generic probe vocabulary beyond current x86-derived probe kinds
  2. `RuntimeParserWalker` / `RuntimeConstructState` extraction from `generated.rs`
  3. `runtime/processors/x86/generated.rs` hard-delete after spine/quirk/provider split is complete

## 2026-04-23 - SLEIGH runtime provider rollback into shared engine

### Summary

- wave type:
  - owner correction / behavior-preserving runtime structure migration
- primary owner:
  - `crates/fission-sleigh/src/runtime/engine.rs`
  - `crates/fission-sleigh/src/runtime/helpers/x86_decode.rs`
  - `crates/fission-sleigh/src/runtime/text/x86.rs`
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- goal:
  - remove `provider` / `quirk` naming that preserved an x86-backend shape
  - restore a shared runtime execution owner
  - separate raw x86 decode helpers from text rendering helpers

### Why this correction was necessary

- `runtime/providers/x86_64.rs` was still effectively an x86-specific backend entrypoint, just under a new name
- `runtime/quirks/x86_fields.rs` owned more than raw field extraction:
  - candidate bucket selection
  - register naming
  - memory operand text formatting
  - Jcc suffix rendering
- this did not match the intended Ghidra-like ownership split of shared execution spine plus narrow ISA-specific helpers

### What changed

- deleted:
  - `crates/fission-sleigh/src/runtime/providers/mod.rs`
  - `crates/fission-sleigh/src/runtime/providers/x86_64.rs`
  - `crates/fission-sleigh/src/runtime/quirks/mod.rs`
  - `crates/fission-sleigh/src/runtime/quirks/x86_fields.rs`
  - `crates/fission-sleigh/src/runtime/processors/mod.rs`
- added shared execution owner:
  - `crates/fission-sleigh/src/runtime/engine.rs`
- added x86 decode helper boundary:
  - `crates/fission-sleigh/src/runtime/helpers/mod.rs`
  - `crates/fission-sleigh/src/runtime/helpers/x86_decode.rs`
- added text projection boundary:
  - `crates/fission-sleigh/src/runtime/text/mod.rs`
  - `crates/fission-sleigh/src/runtime/text/x86.rs`
- renamed runtime dispatch vocabulary:
  - `ExecutionProviderKey` -> `ExecutionEngineKey`
  - `execution_provider_key` -> `execution_engine_key`
  - `executable_provider_key_for_entry(...)` -> `executable_engine_key_for_entry(...)`
- `RuntimeSleighFrontend::{decode_instruction_with_len, decode_and_lift_with_len}` now dispatch through:
  - registry lookup
  - shared `runtime::engine`
  - x86 generated runtime path as the first executable consumer
- `crates/fission-sleigh/src/runtime/processors/x86/generated.rs` now consumes:
  - raw x86 decode helpers from `runtime/helpers/x86_decode.rs`
  - x86 text rendering helpers from `runtime/text/x86.rs`

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: passed
- `git diff -- crates/fission-sleigh/generated`
  - result: empty
- routing cleanup audit:
  - `rg -n "providers::|quirks::|ExecutionProviderKey|execution_provider_key|runtime/processors/x86/mod.rs|processors::x86::" crates/fission-sleigh/src/runtime crates/fission-sleigh/src/lib.rs`
  - result: `0` matches
- CLI smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x1400013e0 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001400 --json`
  - result: passed

### Result

The runtime no longer presents the x86 execution path as a provider/quirk split. The canonical routing owner is back to a shared execution engine, while x86-specific code is narrowed into decode helpers and text projection helpers. This is still not the final Ghidra-style end state, but it removes the misleading architectural shape introduced by the previous wave.

### Remaining risk / next owner

- `crates/fission-sleigh/src/runtime/processors/x86/generated.rs` still owns semantic/runtime behavior that belongs in the shared spine:
  - constructor binding walk
  - template evaluation
  - p-code emission helpers
- `candidate_bucket_keys` remains in the x86 decode helper layer and should eventually disappear into generic decision-tree traversal
- next owner remains:
  1. move `RuntimeParserWalker` / `RuntimeConstructState` behavior out of `generated.rs`
  2. move template evaluation and emission primitives into `runtime/spine`
  3. hard-delete the remaining `runtime/processors/x86/generated.rs`

## 2026-04-23 - SLEIGH architecture-hardcoded helper/text removal

### Summary

- wave type:
  - owner correction / behavior-preserving hardcoding containment
- primary owner:
  - `crates/fission-sleigh/src/runtime/engine.rs`
  - `crates/fission-sleigh/src/runtime/registry.rs`
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- goal:
  - remove standalone architecture-specific helper/text modules from the runtime layer
  - avoid presenting x86-specific decode/text policy as reusable runtime architecture
  - contain the remaining x86-specific compatibility code inside the single known deletion target

### What changed

- deleted:
  - `crates/fission-sleigh/src/runtime/helpers/mod.rs`
  - `crates/fission-sleigh/src/runtime/helpers/x86_decode.rs`
  - `crates/fission-sleigh/src/runtime/text/mod.rs`
  - `crates/fission-sleigh/src/runtime/text/x86.rs`
- moved the temporary x86 decode compatibility code back into:
  - `crates/fission-sleigh/src/runtime/processors/x86/generated.rs`
- removed hardcoded text rendering tables from runtime modules:
  - register name table
  - x86 memory operand string formatter
  - Jcc suffix table
- decode text now uses generic placeholders until SLEIGH display template execution is implemented:
  - constructor mnemonic projection
  - `reg<size>_<index>` register projection
  - structural memory operand projection
- renamed the executable dispatch key:
  - `ExecutionEngineKey::GeneratedX86_64` -> `ExecutionEngineKey::CompiledTable`
- executable readiness still comes from the checked-in manifest; the engine key is no longer named after x86-64

### Validation

- `cargo check -p fission-sleigh`
  - result: passed
- `cargo test -p fission-sleigh -- --test-threads=1`
  - result: `35 passed / 0 failed`
- `cargo check -p fission-cli`
  - result: passed
- `cargo build -p fission-cli --release`
  - result: passed
- `cargo run -p fission-sleigh --example generate_sleigh_frontends`
  - result: passed
- `git diff -- crates/fission-sleigh/generated`
  - result: empty
- hardcoded helper/text audit:
  - `rg -n "runtime/text|text::x86|runtime/helpers|helpers::x86_decode|GeneratedX86_64|x86_decode\\.rs|text/x86\\.rs" crates/fission-sleigh/src`
  - result: `0` matches
- CLI smoke:
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x1400013e0 --json`
  - result: passed
  - `target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001400 --json`
  - result: passed

### Result

The runtime layer no longer has standalone `helpers/x86_decode.rs` or `text/x86.rs` files. This intentionally avoids treating x86-specific prefix/ModRM/SIB parsing or x86 disassembly text formatting as reusable runtime architecture. The remaining x86-specific code is contained in `runtime/processors/x86/generated.rs`, which is now the explicit compatibility holdout and next deletion target.

### Remaining risk / next owner

- this is not full Ghidra parity yet:
  - `generated.rs` still contains x86-specific prefix / ModRM / SIB / displacement parsing
  - `generated.rs` still contains constructor binding and p-code emission policy
- next owner is no longer another rename:
  1. compiler must emit executable token/context/register/display/template IR
  2. shared `runtime/spine` must consume that IR for `DecisionNode`, `ParserWalker`, and `PcodeEmit`
  3. `runtime/processors/x86/generated.rs` must be deleted once the compiled-table path is executable without x86-specific Rust policy
