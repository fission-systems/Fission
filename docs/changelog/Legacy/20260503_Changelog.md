# 2026-05-03 Changelog

## Decompiler quality cycle: sqlite3 sample row admission

- Fixed `full_benchmark` seeded function selection so explicit `canonical_quality_rows`
  are admitted even when a `seed_limit` is active.
- Fixed row-fidelity target normalization in the single-binary benchmark path; tuple
  watchlist rows now become actual row-fidelity targets instead of being dropped.
- Replaced stale sqlite3 export/thunk canary addresses with implementation function
  rows that Fission and Ghidra both decompile:
  `0x1800085d0`, `0x180008fd0`, `0x180009d00`.
- Added a benchmark unit test covering required row admission beyond the configured
  seed limit.

## Validation

- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark`
  passed.
- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py benchmark/asm_benchmark/*.py`
  passed.
- `cargo build --release -p fission-cli` passed.
- `cargo test -p fission-pcode call_target -- --test-threads=1` passed.
- `cargo test -p fission-decompiler-core call_target -- --test-threads=1` passed.

## sqlite3 raw p-code canary parity: SIB extent and dynamic COPY

- Root cause:
  sqlite3 canary rows `0x1800085d0` and `0x180008fd0` decoded `mov qword ptr
  [rsp+disp8], rbx` through the correct `.sla ConstructTpl`, but Fission read
  the displacement from the SIB byte (`0x24`) instead of the following `disp8`
  byte. After fixing the displacement, the remaining difference was that
  Fission folded Ghidra's dynamic-output `COPY temp <- RBX` before the backing
  `STORE`.
- Fix:
  shared-token trailing subtable cursor placement now uses the selected
  terminal `.sla` disjoint-pattern instruction byte length when available.
  This keeps the cursor tied to the verified terminal pattern instead of a
  one-byte ModRM/SIB assumption.
- Fix:
  dynamic memory COPY output emission now follows Ghidra `PcodeEmit` location
  generation: materialize the dynamic output into its temp varnode first, then
  emit the backing STORE from that temp. The parent COPY is no longer folded
  away for register inputs.
- Regression test:
  added `generated_runtime_decodes_sib_stack_disp8_from_sla_terminal_extent`
  for `48 89 5c 24 08`, asserting the `disp8` value `8` comes from after the
  ModRM+SIB terminal extent and that the op shape is `INT_ADD`, `COPY`, `STORE`.
- sqlite3 raw p-code canary:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/sqlite3_decompiler_canary_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/sqlite3_decompiler_canary_dynamic_copy_fixed`
  improved from `full_match=1/3`, average similarity
  `0.6795555555555556`, average parity ratio `0.3333333333333333` to
  `full_match=3/3`, average similarity `1.0`, average parity ratio `1.0`,
  `compat_emitter_used=0`, fake placeholder op `0`, and invalid p-code shape
  `0`.
- x86-64 canonical guard:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/sib_extent_canonical_guard`
  remained green with `full_match=44`, average similarity `1.0`, average
  parity ratio `1.0`, `compat_emitter_used=0`, fake placeholder op `0`, and
  invalid p-code shape `0`.
- sqlite3 full benchmark:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_sib_extent_dynamic_copy_fixed`
  completed with coverage `100%`, failed rows `0`, and weighted average
  normalized similarity `27.640%` (`+0.030` absolute versus the previous
  `27.610%` baseline).
- Validation:
  `cargo test -p fission-sleigh generated_runtime_decodes_sib_stack_disp8_from_sla_terminal_extent -- --test-threads=1 --nocapture`,
  `cargo check -p fission-sleigh`, `cargo build --release -p fission-cli`, and
  `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py`
  passed.

## SLEIGH audit fatal gate rollback and decompiler benchmark revalidation

- Root cause:
  local `crates/fission-sleigh/src/runtime/engine.rs` had promoted reporting-only
  `RuntimeLegacyPathAudit` counters to fatal `UnsupportedGeneratedSemantic`
  errors. This converted otherwise successful `.sla ConstructTpl` executions
  into raw p-code decode failures before NIR/HIR could run.
- Fix:
  restored the default compiled-table contract so legacy audit data remains
  telemetry in `RuntimeExecutionDetails` instead of a semantic failure gate.
  No compatibility emitter, approximation, manual mapping, or architecture
  hardcoding was added.
- Single-row verification:
  `test_functions.exe @ 0x140001450` now decodes as `lea`, length `3`,
  `template_source=SpecDerived`, with audit counters
  `legacy_shared_token_policy=1` and `no_export_subtable_fallback=1` preserved
  as report-only telemetry.
- Canonical raw p-code gate:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/sleigh_preflight_canonical_fixed`
  passed with `full_match=44`, average similarity `1.0`, average parity ratio
  `1.0`, `compat_emitter_used=0`, fake placeholder op `0`, and invalid p-code
  shape `0`.
- sqlite3 raw p-code canary:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/sqlite3_decompiler_canary_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/sqlite3_decompiler_canary_fixed`
  reached all three rows with real `sla_construct_tpl` source. It produced
  `full_match=1/3`, average similarity `0.6795555555555556`, and average
  parity ratio `0.3333333333333333`, so the execution blocker is fixed while
  remaining sqlite3 canary differences are now a separate SLEIGH p-code parity
  owner issue.
- sqlite3 full decompiler benchmark:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_engine_audit_fixed`
  completed with coverage `100%`, failed rows `0`, and weighted average
  normalized similarity `27.610%`.
- Validation:
  `cargo check -p fission-sleigh`, `cargo build --release -p fission-cli`, and
  `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py`
  passed.
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  passed.

## Benchmark

- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_20260503_after`
- Artifact:
  `benchmark/artifacts/full_benchmark/sqlite3_cycle_20260503_after`
- Result:
  coverage recovered to `100.000%`, `20/20` shared successful rows,
  `avg_normalized_similarity=27.610%`, median `29.970%`, aggregate weighted
  similarity `28.250%`.
- Row-fidelity snapshot now reports all three sqlite3 implementation canaries as
  present.

## Notes

- Ghidra 11.4.2 was used as the structure reference, but the local benchmark oracle
  run used Ghidra 12.0.4 because the existing local Ghidra project data was created
  by a newer Ghidra and 11.4.2 failed with `data created with newer version and can
  not be read`.
- Exact call-target owner counters remain `0` on this sqlite3 seed set. The next
  cycle should target indirect/import call target proof instead of broad stack-local
  promotion.
- No benchmark output artifacts, Ghidra DB files, or sample binaries are intended to
  be staged.

## Decompiler Quality V6: exact indirect import call target proof

- Added a separate `NirTypeContext.iat_target_refs` map so callable target VAs and
  exact IAT/import data slots are not conflated.
- Wired loader import/IAT metadata into the decompiler type context as exact
  import-slot identity while keeping direct callable `call_target_refs` separate.
- Added `CALLIND LOAD(const_iat_slot)` proof in HIR call lowering. The path only
  accepts pointer-sized loads from exact IAT slots; non-constant targets, width
  mismatches, and non-IAT loads remain fallback output with typed telemetry.
- Added additive `NirBuildStats` and full benchmark owner metrics for IAT slot
  resolution and indirect-call rejection reasons.
- Fixed full benchmark owner metric aggregation so per-function `preview_build_stats`
  counters are surfaced when the engine summary does not carry that specific alias.

## V6 Validation

- `cargo test -p fission-pcode preview_callind -- --test-threads=1` passed.
- `cargo test -p fission-pcode type_hints_imports -- --test-threads=1` passed.
- `cargo test -p fission-decompiler-core call_target -- --test-threads=1` passed.
- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark`
  passed.
- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py benchmark/asm_benchmark/*.py`
  passed.
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  passed.
- `cargo build --release -p fission-cli` passed.

## V6 Benchmark

- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_indirect_calls_after`
- Artifact:
  `benchmark/artifacts/full_benchmark/sqlite3_cycle_indirect_calls_after`
- Result:
  coverage stayed `100.000%`, `20/20` shared successful rows, and
  `avg_normalized_similarity=27.610%`.
- Owner counters:
  `call_target_indirect_rejected_non_const_ptr=156`,
  `call_proto_unknown_target_kept=58`,
  `call_target_iat_slot_resolved=0`,
  `call_target_indirect_load_resolved=0`.
- Interpretation:
  the sqlite3 canary rows do not currently expose a proofable
  `CALLIND LOAD(const_iat_slot)` path. The new gate now records the exact blocker
  instead of guessing call names from temporary variables or rendered text.

## Decompiler Quality V7: folded indirect import call target proof

- Extended the Ghidra `ActionDeindirect`-style owner in HIR call lowering with a
  strict scalar proof for `CALLIND LOAD(pointer_expr)`.
- Allowed only exact def-use constants: direct constants, `COPY`/cast-preserving
  chains, `INT_ADD`, `INT_SUB`, `PTRADD`, and `PTRSUB` with exact constant inputs.
- Kept register-derived, memory-derived, unknown, unsupported opcode,
  non-dominating, no-def, and ambiguous multi-def cases as fallback output with
  typed telemetry.
- Added additive `NirBuildStats` and full benchmark owner metrics for folded
  pointer proof and rejection breakdown:
  `call_target_indirect_ptr_const_folded`,
  `call_target_indirect_rejected_unsupported_ptr_opcode`,
  `call_target_indirect_rejected_ambiguous_def`,
  `call_target_indirect_rejected_non_dominating_def`, and
  `call_target_indirect_rejected_no_def`.
- Added targeted tests proving `CALLIND LOAD(COPY(IAT_CONST))` and
  `CALLIND LOAD(INT_ADD(base_const, delta_const))` resolve only through exact
  `NirTypeContext.iat_target_refs`; unsupported pointer opcodes do not promote
  API names.

## V7 Validation

- `cargo test -p fission-pcode call_target -- --test-threads=1` passed.
- `cargo test -p fission-pcode type_hints_imports -- --test-threads=1` passed.
- `cargo test -p fission-decompiler-core call_target -- --test-threads=1` passed.
- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark`
  passed.
- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py benchmark/asm_benchmark/*.py`
  passed.
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  passed.
- `cargo build --release -p fission-cli` passed.

## V7 Benchmark

- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_deindirect_v7_after`
- Artifact:
  `benchmark/artifacts/full_benchmark/sqlite3_cycle_deindirect_v7_after`
- Result:
  benchmark command completed, but Fission rows failed before the V7 NIR call
  target path with `UnsupportedGeneratedSemantic: x86-64 runtime status is
  executable_candidate`. The active local worktree contains an unrelated
  unstaged SLEIGH runtime gate change, so this run is not a valid decompiler
  quality regression measurement.
- Cross-check:
  a temporary clean worktree with only the V7 patch applied built successfully,
  but the sqlite3 benchmark still failed before NIR with
  `UnsupportedPcodeTemplate: x86-64: missing_sla_construct_tpl`. This indicates
  the current clean checkout/generated SLEIGH artifact state cannot reproduce
  the previous sqlite3 V6 benchmark lane without restoring the exact generated
  SLEIGH runtime artifact state.
- Interpretation:
  V7 is validated by unit and integration gates, but the sqlite3 full benchmark
  is blocked by SLEIGH artifact/runtime state before call-target lowering. No
  benchmark-side repair or call-name guessing was added.

## Raw P-code SLEIGH preflight gate for decompiler V7

- Added `benchmark/raw_p_code_benchmark/sqlite3_decompiler_canary_rows.json`
  with sqlite3 decompiler canary rows `0x1800085d0`, `0x180008fd0`, and
  `0x180009d00`. This manifest is an admission guard: decompiler quality
  benchmarks should not be interpreted until SLEIGH raw p-code reaches these
  rows first.
- Ran canonical raw p-code preflight:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/sleigh_preflight_canonical_current`
- Result:
  canonical gate failed before parity comparison with `fission_decode_error=17`,
  `missing_fission_instruction=29`, `full_match=0`, average similarity `0.0`,
  and average parity ratio `0.0`. All sampled Fission instruction failures were
  `UnsupportedGeneratedSemantic: x86-64 runtime status is executable_candidate`.
- Ran vendor raw p-code smoke:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/vendor_binary_smoke.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/sleigh_preflight_vendor_current`
- Result:
  vendor smoke failed before parity comparison with `fission_decode_error=4`,
  `missing_fission_instruction=12`, and Fission instruction count `0`. x86 and
  x86-64 rows failed with `UnsupportedGeneratedSemantic: <language> runtime
  status is executable_candidate`.
- Ran sqlite3 raw p-code canary:
  `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/sqlite3_decompiler_canary_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/sqlite3_decompiler_canary_current`
- Result:
  sqlite3 canary failed before NIR/decompiler entry with `fission_decode_error=3`
  and all three rows reporting `UnsupportedGeneratedSemantic: x86-64 runtime
  status is executable_candidate`.
- Interpretation:
  the current local SLEIGH runtime state blocks raw p-code below NIR/HIR, so the
  sqlite3 full benchmark was intentionally not re-run as a V7 quality gate. The
  next owner is SLEIGH runtime/generated artifact gating, not call-target
  decompiler logic.

## Raw P-code preflight validation

- `cargo check -p fission-sleigh` passed.
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  passed.
- `cargo build --release -p fission-cli` passed.
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py`
  passed.
- `cargo test -p fission-pcode type_hints_imports -- --test-threads=1` passed.
- `cargo test -p fission-pcode call_target -- --test-threads=1` passed.
- `cargo test -p fission-decompiler-core call_target -- --test-threads=1` passed.
