# 2026-04-22 Changelog

## Summary

This changelog splits out the latest Windows small C quality-program work from the long cumulative `20260421_Changelog.md`.

The current quality bottleneck is no longer generic structuring. The owner has been narrowed to:

- guarded-tail / BlockGraph alias ownership
- especially `AliasHasNonlocalRef`
- and inside that family, `nested_before`

The two waves recorded here reflect that narrower owner:

1. alias-interleave owner narrowing in benchmark and compact artifacts
2. nested-before alias ownership proof at the canonical guarded-tail owner

---

## 1. Alias-Interleave Owner Narrowing In Benchmark / Compact Artifacts

### Scope

This wave did not broaden guarded-tail acceptance. It narrowed the next quality blocker into explicit benchmark-visible subtype metrics.

Canonical surface:

- runner:
  - `benchmark/full_benchmark/full_decomp_benchmark.py`
- samples:
  - `benchmark/binary/x86-64/window/small/binary/c`
- baseline:
  - `benchmark/artifacts/full_benchmark/windows-small-c-guarded-tail-ownership-latest`
- trial:
  - `benchmark/artifacts/full_benchmark/windows-small-c-alias-interleave-metrics-latest`

Primary owner:

- `benchmark/full_benchmark/grand_finale_support/benchmark_core.py`
- `benchmark/full_benchmark/grand_finale_support/compact_summary.py`

This wave exists to answer one concrete question cleanly:

- when `canonical_alias_interleave_conflict_count` is nonzero, which subtype is actually dominant?

### What changed

Added a dedicated alias-interleave metric family to verbose JSON, compact JSON, Markdown, and console output.

New alias-interleave metric vocabulary:

- `alias_interleave_conflict`
- `alias_has_nonlocal_ref`
- `alias_has_nonlocal_ref_external_before`
- `alias_has_nonlocal_ref_nested_before`
- `alias_has_nonlocal_ref_post_segment_ref`
- `alias_not_fallthrough`
- `alias_not_fallthrough_top_level_after_label`
- `alias_not_fallthrough_nested_after_label`
- `alias_has_multiple_internal_predecessors`
- `payload_crosses_join`

Projection cleanup also fixed one contract gap:

- these counters now flow not only into `alias_interleave_metric_totals`
- they also appear in `failure_family_distribution` as:
  - `canonical_alias_has_nonlocal_ref_count`
  - `canonical_alias_has_nonlocal_ref_external_before_count`
  - `canonical_alias_has_nonlocal_ref_nested_before_count`
  - `canonical_alias_has_nonlocal_ref_post_segment_ref_count`
  - `canonical_alias_not_fallthrough_count`
  - `canonical_alias_has_multiple_internal_predecessors_count`
  - `canonical_payload_crosses_join_count`

This is important because the prior state exposed `canonical_alias_interleave_conflict_count`, but not the internal cause distribution behind it.

### Validation

Python contract:

- `python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py`
- `python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py`

Rust / CLI contract:

- `cargo check -p fission-pcode`
- `cargo check -p fission-automation`
- `cargo build -p fission-cli --release`
- `cargo test -p fission-pcode -- --test-threads=1`

Result:

- `664 passed / 0 failed`

### Windows small C 2-way benchmark

Corpus quality remained neutral:

```text
weighted_avg_normalized_similarity: 37.604286 -> 37.604286
x64 weighted_avg_normalized_similarity: 37.604 -> 37.604
new failed rows: 0
row gates: passed for all 6 binaries
promotion_blockers: advisory_gate_mode
```

BlockGraph totals remained unchanged from the previous guarded-tail ownership wave:

```text
candidate: 414 -> 414
complete: 0 -> 0
rejected_must_emit_label: 414 -> 414
rejected_external_ref: 108 -> 108
rejected_join_owner_conflict: 128 -> 128
rejected_middle_ref: 24 -> 24
```

New alias-interleave totals are now first-class in the compact artifact:

```text
alias_interleave_conflict: 50
alias_has_nonlocal_ref: 56
alias_has_nonlocal_ref_external_before: 18
alias_has_nonlocal_ref_nested_before: 24
alias_has_nonlocal_ref_post_segment_ref: 2
alias_not_fallthrough: 9
alias_not_fallthrough_top_level_after_label: 6
alias_not_fallthrough_nested_after_label: 3
alias_has_multiple_internal_predecessors: 0
payload_crosses_join: 0
```

The practical reading is now much clearer:

- the dominant alias-interleave owner is not generic
- it is `AliasHasNonlocalRef`
- and inside that family the largest subtype is `nested_before`

Representative binary-level readout:

```text
test_functions.exe
- alias_has_nonlocal_ref: 5
- external_before: 2
- nested_before: 2

array_operations.exe
- alias_interleave_conflict: 12
- alias_has_nonlocal_ref: 12
- external_before: 6
- nested_before: 4
- alias_not_fallthrough_nested_after_label: 3

math_operations.exe
- alias_interleave_conflict: 16
- alias_has_nonlocal_ref: 15
- external_before: 4
- nested_before: 6
- post_segment_ref: 2
- alias_not_fallthrough_top_level_after_label: 6
```

### What improved

Concrete improvement in this wave:

- the compact AI-facing artifact is now short but owner-complete for the next guarded-tail blocker
- `failure_family_distribution` and compact summary now agree on alias-interleave subtype totals
- the next semantic owner is narrower than before:
  - `AliasHasNonlocalRef`
  - especially `nested_before`

This is a real improvement over the previous state because the previous artifact still forced manual trace reading to separate:

- `nested_before`
- `external_before`
- `post_segment_ref`
- `alias_not_fallthrough`

### What did not improve

This wave intentionally did not change decompiler semantics.

So the expected non-improvements are:

- `fibonacci` remains linearized
- corpus similarity is unchanged
- `blockgraph_region_complete_count` remains `0`
- no control-heavy sample row quality uplift yet

### Duplicate-logic audit

No new semantic repair layer was introduced.

The change stayed in benchmark/reporting ownership:

- no printer-side repair
- no CLI-side repair
- no duplicate telemetry vocabulary outside the benchmark support modules

### Final status

```text
wave_type: quality-neutral owner-narrowing
primary_owner: benchmark/compact alias-interleave reporting
behavior_changed: no
release_path_changed: no
env_gate: none
promotion impact: unchanged
next owner: guarded-tail canonicalization proof for AliasHasNonlocalRef nested_before
```

---

## 2. Windows Small C Quality Wave: nested-before alias ownership proof

### Summary

This wave implemented the first semantic slice of the Ghidra FlowBlock clean-room migration for `AliasHasNonlocalRef nested_before`.

The change stayed at the canonical structuring owner:

- `crates/fission-pcode/src/nir/structuring/guarded_tail/`
- no printer-side repair
- no CLI-side repair
- no benchmark-script semantic patching

The concrete change was narrow:

- guarded-tail canonicalization no longer treats every `external_nested_before > 0` as the same hard reject
- same-guard-family nested conditional refs and paired nested-boundary refs now get a typed ownership proof before rejection
- all other nested-before shapes remain fail-closed

### Implementation

New clean-room proof vocabulary was added for the nested-before owner:

- `AliasOwnershipProof`
- `NestedBeforeAliasWitness`
- `NestedBeforeOwnershipClass`
- `AliasOwnershipLegalityReason`

Allowed proof-complete classes:

- `GuardFamilyInternalizable`
- `PairedBoundaryInternalizable`

Fail-closed classes retained:

- `NestedBeforeExternalOwner`
- `NestedBeforeNonlocalPayload`
- `NestedBeforeUnknown`

The key architectural change is ownership reuse:

- canonicalization now consumes suffix-window guard-family / paired-boundary proof helpers
- the nested-before classification logic stays inside guarded-tail ownership
- duplicate semantic logic was reduced instead of adding a second ad hoc path

### Validation

Rust validation passed:

```text
cargo test -p fission-pcode suffix_window -- --test-threads=1
- 63 passed / 0 failed

cargo test -p fission-pcode structuring_candidate_discovery_ -- --test-threads=1
- 52 passed / 0 failed

cargo test -p fission-pcode -- --test-threads=1
- 667 passed / 0 failed

cargo check -p fission-pcode
- passed

cargo check -p fission-automation
- passed

cargo build -p fission-cli --release
- passed
```

New synthetic positive coverage was added:

- same-guard-family nested-before alias ownership now internalizes
- paired nested-boundary alias ownership now internalizes

### Windows small C 2-way benchmark

Same-axis corpus result remained neutral:

```text
weighted_avg_normalized_similarity: 37.604286 -> 37.604286
new failed rows: 0
promotion_blockers: advisory_gate_mode
```

Alias-interleave totals were unchanged:

```text
alias_has_nonlocal_ref: 56 -> 56
alias_has_nonlocal_ref_nested_before: 24 -> 24
alias_has_nonlocal_ref_external_before: 18 -> 18
alias_has_nonlocal_ref_post_segment_ref: 2 -> 2
alias_interleave_conflict: 50 -> 50
```

BlockGraph totals also remained unchanged:

```text
candidate: 414 -> 414
complete: 0 -> 0
rejected_must_emit_label: 414 -> 414
rejected_external_ref: 108 -> 108
rejected_join_owner_conflict: 128 -> 128
rejected_middle_ref: 24 -> 24
```

Representative target row remained unchanged:

```text
test_functions.exe:fibonacci @ 0x140001470
- forced_linear_structuring_count: 1 -> 1
- rendered_code_len: 40935 -> 40935
```

### Reading of the result

What improved:

- the semantic owner is now narrower at the canonical guarded-tail layer
- synthetic same-guard-family / paired-boundary shapes now have a typed acceptance path
- duplicate logic between suffix-window proof and canonicalization was reduced

What did not improve:

- the current Windows small C corpus did not exercise the newly admitted proof-complete family strongly enough to move benchmark metrics
- no measured row quality changed
- `blockgraph_region_complete_count` is still `0`

This means the next owner is not generic nested-before anymore. It is the unresolved remainder inside that family:

- `NestedBeforeExternalOwner`
- and then `MustEmitLabel` join/follow ownership after that

### Final status

```text
wave_type: behavior-changing semantic trial
primary_owner: guarded-tail canonicalization nested-before alias ownership
behavior_changed: yes
release_path_changed: no
env_gate: none
promotion impact: neutral on current corpus
next owner: NestedBeforeExternalOwner -> MustEmitLabel join/follow ownership
```

---

## 3. Benchmark Measurement Readout Tightening For Target Structuring Rows

### Summary

This follow-up wave checked whether the benchmark was failing to measure quality movement on the current Windows small C owner.

The answer is:

- the benchmark was **not blind** to `fibonacci`
- the target row was already being measured by row-fidelity gate
- but the compact/Markdown artifact was too coarse to show that quickly
- and one target-row selector path was overmatching a shared bare address across binaries

### What changed

Two additive reporting fixes were introduced.

First, target structuring rows now carry baseline/current quality readout:

- `previous_normalized_similarity`
- `current_normalized_similarity`
- `normalized_similarity_delta`
- `row_gate_status`
- `watchlist_role`
- `failure_reasons`

This is now projected through:

- corpus compact summary
- single-binary compact summary
- corpus Markdown
- single-binary Markdown

Second, the target-row selector no longer overmatches a shared address globally across the corpus.

Previous bug:

- `0x140001470` was treated as a global target address
- this incorrectly pulled `function_pointers_and_strings.exe:compare_int_descending`
  into `target_structuring_rows`

Current contract:

- name-based target rows remain supported for:
  - `fibonacci`
  - `fibonacci_memo`
- address-based targeting is now binary-scoped for the known canary row

### Validation

Python contract passed:

```text
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
- 25 passed / 0 failed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
- 7 passed / 0 failed
```

Added checks:

- target structuring rows inherit row-gate delta fields
- shared-address rows from unrelated binaries no longer overmatch

### Windows small C benchmark readout

New artifact:

- `benchmark/artifacts/full_benchmark/windows-small-c-target-row-delta-latest/benchmark_compact_summary.json`

The updated compact readout now shows the real state directly:

```text
test_functions:fibonacci @ 0x140001470
- current_normalized_similarity: 11.65
- previous_normalized_similarity: 11.65
- normalized_similarity_delta: 0.00
- row_gate_status: unchanged
- forced_linear_structuring_count: 1

math_operations:fibonacci_memo @ 0x140001a90
- current_normalized_similarity: 15.36
- previous_normalized_similarity: 15.36
- normalized_similarity_delta: 0.00
- row_gate_status: unchanged
```

Important conclusion:

- the benchmark was already measuring the current canary rows correctly
- the current semantic wave simply did not change their emitted pseudocode
- the measurement gap was primarily artifact readability and target-row selector precision

Corpus headline remains unchanged:

```text
weighted_avg_normalized_similarity: 37.604286 -> 37.604286
new failed rows: 0
```

### Final status

```text
wave_type: additive benchmark contract tightening
primary_owner: benchmark target-row readout and selector precision
behavior_changed: no decompiler semantic change
release_path_changed: no
env_gate: none
practical_result: benchmark now shows target-row no-change explicitly instead of forcing manual row-gate inspection
next owner: NestedBeforeExternalOwner semantic acceptance, not more telemetry
```

### Follow-up completion: canonical quality rows + code hash + verbose summary parity

The initial target-row delta tightening exposed one real implementation gap:

- `build_comparison()` was wired for `canonical_quality_rows` only partially
- single-binary verbose summaries were written before `target_structuring_rows` were re-annotated from baseline row-gate
- as a result, compact summary had the right answer, but single-binary verbose summary could still hide unchanged target rows

That follow-up is now closed.

Additional changes:

- manifest-owned `canonical_quality_rows` added to:
  - `benchmark/config/benchmark_corpus/windows_small_c_samples.json`
- row-fidelity snapshot now preserves:
  - `canonical_quality_rows`
  - `canonical_quality_row_count`
- target rows now project:
  - `current_code_sha256`
  - `previous_code_sha256`
  - `code_changed`
- compact summary now emits:
  - `unchanged_target_rows`
- verbose corpus + verbose single benchmark summaries now also emit:
  - `Unchanged Target Rows`

### Validation

Python contracts after the follow-up:

```text
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
- 27 passed / 0 failed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
- 7 passed / 0 failed
```

Revalidated artifact:

- `benchmark/artifacts/full_benchmark/windows-small-c-target-row-delta-latest/benchmark_compact_summary.json`

Representative target rows now show explicit no-change evidence:

```text
test_functions:fibonacci @ 0x140001470
- current_normalized_similarity: 11.65
- previous_normalized_similarity: 11.65
- normalized_similarity_delta: 0.00
- code_changed: false
- current_code_sha256 == previous_code_sha256

math_operations:fibonacci_memo @ 0x140001a90
- current_normalized_similarity: 15.36
- previous_normalized_similarity: 15.36
- normalized_similarity_delta: 0.00
- code_changed: false
- current_code_sha256 == previous_code_sha256
```

Corpus headline remains:

```text
weighted_avg_normalized_similarity: 37.604286
new_failed_rows: 0
```

Interpretation:

- the benchmark was not under-measuring the current quality wave
- the canary rows truly did not change
- the remaining bottleneck is semantic owner movement, not benchmark visibility

### Final status for today

```text
wave_type: benchmark contract tightening
primary_owner: target-row measurement fidelity
behavior_changed: no decompiler semantic change
release_path_changed: no
env_gate: none
artifact_contract_added:
  - canonical_quality_rows
  - canonical_quality_row_count
  - current_code_sha256
  - previous_code_sha256
  - code_changed
  - unchanged_target_rows
next owner: BlockGraph / guarded-tail semantic acceptance, not benchmark instrumentation
```

---

## 4. Guarded-Tail Join-Owner Reclassification At Must-Emit-Label Gate

### Summary

This wave implemented the next narrow clean-room migration slice under the canonical guarded-tail owner:

- owner: `crates/fission-pcode/src/nir/structuring/guarded_tail/`
- focus: `MustEmitLabel` gate classification
- scope: same-guard-family nested-before refs before a guarded-tail candidate

The goal was not to broaden structuring generally. It was to stop collapsing all pre-candidate nested refs into the same external-reference bucket.

### What changed

The `MustEmitLabel` gate now classifies outside refs with more structure:

- top-level-before
- nested-before
- top-level-after
- nested-after

Narrow admission added:

- same-guard-family nested-before refs are internalized for join-owner accounting
- unrelated nested-before refs stay fail-closed and are now counted as `owner_conflict`

Concrete implementation points:

- `promotion.rs`
  - replaced the old raw `outside_refs` threshold logic with typed site accounting
- `promotion_graph.rs`
  - added `internalized_guard_family_nested_before_refs_for_join_owner(...)`
- `suffix_window.rs`
  - reused `exprs_share_guard_family(...)` across the guarded-tail owner
- `structuring_guarded_tail.rs`
  - updated the discovery regression to expect `owner_conflict` for the unresolved nested-before family

### Validation

Rust validation for this wave:

```text
cargo test -p fission-pcode must_emit_label_internalizes_same_guard_family_nested_before_owner -- --nocapture
- passed

cargo test -p fission-pcode must_emit_label_rejects_unrelated_nested_before_owner -- --nocapture
- passed

cargo test -p fission-pcode structuring_candidate_discovery_ -- --test-threads=1
- 52 passed / 0 failed

cargo test -p fission-pcode suffix_window -- --test-threads=1
- 63 passed / 0 failed

cargo check -p fission-pcode
- passed

cargo check -p fission-automation
- passed

cargo build -p fission-cli --release
- passed
```

### Windows small C 2-way benchmark

Baseline:

- `benchmark/artifacts/full_benchmark/windows-small-c-target-row-delta-latest`

Trial:

- `benchmark/artifacts/full_benchmark/windows-small-c-join-owner-latest`

Corpus headline:

```text
weighted_avg_normalized_similarity: 37.604286 -> 37.604286
new failed rows: 0
release_promotion_allowed: false -> false
```

What moved:

```text
blockgraph candidate: 414 -> 418
blockgraph rejected_external_ref: 108 -> 0
blockgraph rejected_join_owner_conflict: 128 -> 232
blockgraph rejected_must_emit_label: 414 -> 418
```

What did not move:

```text
alias_has_nonlocal_ref: 56 -> 56
alias_has_nonlocal_ref_nested_before: 24 -> 24
alias_has_nonlocal_ref_external_before: 18 -> 18
alias_interleave_conflict: 50 -> 50
```

Representative canary rows remained byte-stable:

```text
test_functions:fibonacci @ 0x140001470
- current_normalized_similarity: 11.65 -> 11.65
- code_changed: false
- forced_linear_structuring_count: 1 -> 1

math_operations:fibonacci_memo @ 0x140001a90
- current_normalized_similarity: 15.36 -> 15.36
- code_changed: false
```

### Reading of the result

This wave produced a real owner shift, but not a quality uplift.

What improved:

- `MustEmitLabel` no longer hides unresolved nested-before cases inside `rejected_external_ref`
- the benchmark proves the current bottleneck is classification/ownership, not measurement blindness
- same-guard-family nested-before refs now have an explicit acceptance path at the promotion gate

What did not improve:

- no current Windows small C row changed rendered output
- `blockgraph_region_complete_count` is still `0`
- `fibonacci` is still linearized

Practical interpretation:

- the benchmark is measuring correctly
- the current wave reclassified the remaining blocker more accurately
- the next owner is the unresolved `join_owner_conflict` family, not generic benchmark infrastructure

### Duplicate-logic audit

The change stayed inside the canonical guarded-tail owner:

- no printer-side repair
- no CLI-side repair
- no benchmark-script semantic patching

Remaining duplication status:

- guard-family relation checking is shared via `exprs_share_guard_family(...)`
- join-owner pre-candidate scanning still exists as a promotion-gate-specific helper because the promotion window is candidate-anchored, not alias-segment-anchored

### Final status

```text
wave_type: behavior-changing owner reclassification
primary_owner: guarded-tail MustEmitLabel join-owner gate
behavior_changed: yes
release_path_changed: no
env_gate: none
promotion impact: neutral on current Windows small C corpus
next owner: BlockGraph join/follow ownership completion for unresolved join_owner_conflict
```

---

## 5. CLI Process CPU / Worker Telemetry For Parallel Decompilation

### Summary

This wave added first-class process CPU and worker readout to the CLI benchmark JSON path so hot-machine / fan-noise reports can be separated from actual Fission CPU behavior.

The immediate investigation found:

- no active `fission_cli`, benchmark, Ghidra Java, cargo, or rustc process during the heat report
- the dominant CPU process was VS Code `Code Helper (Plugin)` PID `19578`
- that process was tied to the Continue extension and `.continue/index/index.sqlite-shm`

So the current heat event was not caused by a running Fission decompilation job.

### What changed

`fission_cli decomp ... --json --benchmark` now emits additive `_meta` fields:

- `worker_count`
- `worker_fanout_enabled`
- `available_parallelism`
- `worker_env_requested`
- `decomp_stack_mb`
- `cpu_user_sec`
- `cpu_system_sec`
- `cpu_total_sec`
- `cpu_utilization_pct`
- `effective_parallelism`

The CPU values are process-local and come from `getrusage(RUSAGE_SELF)` on Unix/macOS. They are intentionally independent from the benchmark wrapper sampler, which can miss short Fission runs.

Benchmark summaries now project those values into:

- verbose single-binary summary
- verbose corpus summary
- compact single-binary summary
- compact corpus summary
- per-binary compact rows

### Validation

Rust / Python validation:

```text
cargo check -p fission-cli
- passed

cargo build -p fission-cli --release
- passed

cargo check -p fission-automation
- passed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
- 27 passed / 0 failed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
- 7 passed / 0 failed
```

Live worker scaling probe:

Target:

- `benchmark/binary/x86-64/window/small/binary/c/array_operations.exe`

Command shape:

- `fission_cli decomp <binary> --all --json --benchmark --include-nonuser-functions`

Measured `_meta` results:

```text
workers=1
- function_count: 127
- wall_clock_sec: 1.441615
- cpu_total_sec: 1.289221
- cpu_utilization_pct: 89.429
- effective_parallelism: 0.894
- total_decomp_sec: 1.422880

workers=2
- function_count: 127
- wall_clock_sec: 1.160407
- cpu_total_sec: 1.275793
- cpu_utilization_pct: 109.944
- effective_parallelism: 1.099
- total_decomp_sec: 1.327213

workers=4
- function_count: 127
- wall_clock_sec: 1.096661
- cpu_total_sec: 1.379144
- cpu_utilization_pct: 125.758
- effective_parallelism: 1.258
- total_decomp_sec: 1.403092

workers=8
- function_count: 127
- wall_clock_sec: 1.129866
- cpu_total_sec: 1.773962
- cpu_utilization_pct: 157.006
- effective_parallelism: 1.570
- total_decomp_sec: 2.042907
```

### Reading of the result

What improved:

- Fission now reports its own process CPU consumption and parallelism directly in benchmark JSON.
- The compact AI-facing artifact can distinguish wall-clock speed from total CPU burn.
- Worker fan-out can now be evaluated as a throughput/CPU-efficiency tradeoff instead of guessed from wall time.

What did not change:

- no decompiler semantic behavior changed
- no worker scheduling policy changed
- no quality metric changed

Practical interpretation:

- current small C corpus runs are not showing an infinite-loop / runaway CPU signature
- the workload has limited parallel efficiency at high worker counts
- `workers=4` gave the best wall time in this probe
- `workers=8` increased total CPU seconds and effective parallelism but did not improve wall time

### Final status

```text
wave_type: diagnostic/reporting
primary_owner: fission-cli benchmark metadata + benchmark compact projection
behavior_changed: no decompiler semantic change
release_path_changed: no
env_gate: none
practical_result: CPU usage is now measurable per Fission run; current heat report points outside Fission
next owner: adaptive worker policy and per-function progress guards
```

## 6. MIR Shadow Contract Wave 1

### Scope

This wave introduces the first internal MIR contract between current NIR/HIR normalization and final HIR printing.

```text
wave_type: behavior-preserving shadow contract
primary_owner: crates/fission-pcode/src/nir/mir
behavior_changed: no semantic output change intended
release_path_changed: no
env_gate: none
```

The new MIR layer is intentionally shadow-only. It projects the current normalized `HirFunction` into `MirFunction` for telemetry and ownership-boundary validation, but does not round-trip back into HIR and does not feed the printer.

### Added Internal Contract

New internal owner:

- `crates/fission-pcode/src/nir/mir/`

Initial MIR concepts:

- `MirFunction`
- `MirBlock`
- `MirValueId`
- `MirStmt`
- `MirTerminator`
- `MirValueKind`
- `MirMemoryRegion`
- `MirJoinProof`
- `MirRegionProof`
- `MirLoweringStats`

Telemetry added to `NirBuildStats`:

- `mir_enabled_count`
- `mir_function_count`
- `mir_block_count`
- `mir_value_count`
- `mir_memory_region_count`
- `mir_join_proof_count`
- `mir_region_proof_count`
- `mir_projection_duration_ms`

Benchmark reporting now exposes MIR metrics in:

- verbose single-binary summary
- verbose corpus summary
- compact single-binary summary
- compact corpus summary
- console summary
- per-binary compact rows

### Validation

Rust / Python validation:

```text
cargo test -p fission-pcode mir_ -- --test-threads=1
- passed: 3 passed / 0 failed

cargo test -p fission-pcode -- --test-threads=1
- passed: 672 passed / 0 failed

cargo check -p fission-pcode
- passed

cargo check -p fission-automation
- passed

cargo build -p fission-cli --release
- passed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
- passed: 28 passed / 0 failed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
- passed: 7 passed / 0 failed
```

Live targeted validation:

Target:

- `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe`
- function: `fibonacci @ 0x140001470`

Observed `preview_build_stats`:

```text
mir_enabled_count: 1
mir_function_count: 1
mir_block_count: 1
mir_value_count: 3639
mir_memory_region_count: 0
mir_join_proof_count: 86
mir_region_proof_count: 21
mir_projection_duration_ms: 0
rendered code length: 40935
```

Windows small C 2-way benchmark:

```text
baseline: benchmark/artifacts/full_benchmark/windows-small-c-nested-before-ownership-latest
trial: benchmark/artifacts/full_benchmark/windows-small-c-mir-latest
weighted_avg_normalized_similarity: 37.604%
x64 weighted_avg_normalized_similarity: 37.604%
row gates: 6 / 6 passed
direct_success: 6 / 6 non-worse
coverage: 6 / 6 non-worse
target rows code_changed: false for fibonacci and fibonacci_memo
```

MIR corpus totals:

```text
enabled: 294
function: 294
block: 294
value: 9066
memory_region: 324
join_proof: 658
region_proof: 407
projection_duration_ms: 0
```

### Reading of the Result

What improved:

- Fission now has a concrete internal MIR owner boundary that maps to Ghidra-style `Funcdata` / `Heritage` / `BlockGraph` working-state responsibilities.
- Benchmark artifacts can now confirm whether MIR projection is active and how much semantic surface it sees.
- The compact AI-facing artifact now exposes `mir_metric_totals`, so future MIR materialization or BlockGraph experiments can be compared without opening full artifacts.

What did not change:

- no CLI syntax changed
- no HIR output path changed
- no representative/materialization policy moved yet
- no BlockGraph acceptance changed yet
- sample quality stayed neutral rather than improving

Promotion status:

```text
status: blocked
reason: corpus gate reported failure_family_distribution canonical_must_emit_label_conflict_count 1088 -> 1092
interpretation: this wave is shadow-only and target rows remained code_changed=false, so the blocker is treated as existing/parallel failure-family drift rather than MIR output mutation
```

### Next Owner

The next MIR wave should not broaden structuring heuristics. It should move one narrow representative/materialization read-only decision into MIR behind an env gate:

- `FISSION_ENABLE_MIR_MATERIALIZATION`
- target metric: `materialization_stabilized_count`
- acceptance: weighted similarity neutral, new failed rows `0`, and no increase in `canonical_must_emit_label_conflict_count`

## 7. MIR BlockGraph Admission Gate Trial

### Summary

This wave added a default-off MIR BlockGraph behavior trial for the Ghidra `FlowBlock` / `BlockGraph` clean-room migration path.

Fixed metadata:

```text
wave_type: env-gated behavior trial
env_gate: FISSION_ENABLE_MIR_BLOCKGRAPH
default_status: off
primary_owner: MIR BlockGraph / structuring admission
release_path_changed: no
```

Implementation:

- Added MIR BlockGraph contract types:
  - `MirBlockGraph`
  - `MirRegionCandidate`
  - `MirJoinOwnershipProof`
  - `MirFollowOwnershipProof`
- Added additive `NirBuildStats` telemetry:
  - `mir_blockgraph_admission_enabled_count`
  - `mir_blockgraph_irreducible_budget_bypass_count`
  - `mir_blockgraph_extreme_budget_blocked_count`
- Projected the new MIR admission telemetry into verbose and compact benchmark summaries.
- Added an env-gated admission path where `IrreducibleBudget` can attempt graph-collapse under `FISSION_ENABLE_MIR_BLOCKGRAPH=1`.
- Kept `ExplicitForceLinear` and `ExtremeBudget` fail-closed.
- Added fail-closed fallback when the MIR BlockGraph trial produces no complete region proof.

### Targeted Validation

Target:

- `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe`
- `fibonacci @ 0x140001470`

Env-off:

```text
wall_clock_sec: 0.041012
forced_linear_structuring_count: 1
structuring_force_linear_irreducible_budget_count: 1
mir_blockgraph_admission_enabled_count: 0
blockgraph_region_candidate_count: 0
blockgraph_region_complete_count: 0
rendered_code_len: 40935
code_sha256: 3b7597bb307cb3155faba1c9d19b518558b6ce5597ec99fac53d929b0f8ab313
```

Env-on:

```text
wall_clock_sec: 0.114046
forced_linear_structuring_count: 1
structuring_force_linear_irreducible_budget_count: 1
mir_blockgraph_admission_enabled_count: 1
mir_blockgraph_irreducible_budget_bypass_count: 1
blockgraph_region_candidate_count: 4
blockgraph_region_complete_count: 0
blockgraph_region_rejected_must_emit_label_count: 4
rendered_code_len: 40843
code_sha256: f3f0bafe536ad2a2ce90f174eed8e254b26ea11dc55783acccc2927371eea92f
```

Interpretation:

- The env gate reaches the MIR BlockGraph admission path.
- It does not yet produce a complete BlockGraph region.
- `fibonacci` remained forced-linear after fail-closed fallback.
- The fallback is not byte-stable yet because the graph-collapse attempt still mutates builder state before returning to linear output.
- The target row similarity moved only `11.65 -> 11.66`, so this is not a meaningful quality uplift.

### Windows Small C 2-Way Benchmark

Benchmark:

```text
baseline: benchmark/artifacts/full_benchmark/windows-small-c-mir-latest
trial: benchmark/artifacts/full_benchmark/windows-small-c-mir-blockgraph-latest
artifact: benchmark/artifacts/full_benchmark/windows-small-c-mir-blockgraph-latest/benchmark_compact_summary.json
```

Corpus result:

```text
weighted_avg_normalized_similarity: 37.604285714285716
x64 weighted_avg_normalized_similarity: 37.604285714285716
new failed rows: none
row gates: 6 / 6 passed
release_promotion_allowed: false
```

Promotion blockers:

```text
advisory_gate_mode
failure_family_distribution canonical_must_emit_label_conflict_count: 1092 -> 1100
failure_family_distribution canonical_alias_interleave_conflict_count: 50 -> 64
failure_family_distribution canonical_emit_ready_failed_count: 1022 -> 1028
```

MIR totals:

```text
enabled: 294
function: 294
block: 294
value: 9010
join_proof: 658
region_proof: 407
blockgraph_admission_enabled: 208
blockgraph_irreducible_budget_bypass: 1
blockgraph_extreme_budget_blocked: 0
```

BlockGraph totals:

```text
candidate: 422
complete: 0
rejected_must_emit_label: 422
rejected_join_owner_conflict: 232
rejected_middle_ref: 24
```

### Reading of the Result

What improved:

- MIR now has explicit BlockGraph join/follow ownership contract types.
- The benchmark can distinguish MIR BlockGraph admission from ordinary shadow MIR projection.
- The trial proves the next blocker is still not admission itself, but completion of join/follow ownership proof.

What regressed or stayed blocked:

- `blockgraph_region_complete_count` stayed `0`.
- `canonical_must_emit_label_conflict_count`, `canonical_alias_interleave_conflict_count`, and `canonical_emit_ready_failed_count` increased under env-on trial.
- `fibonacci` still has no meaningful readability uplift.
- The env-on `fibonacci` output changed despite fail-closed fallback, so future work must snapshot or isolate MIR BlockGraph trial state before admission can be trusted.

Promotion status:

```text
status: blocked
reason: env-gated trial did not produce complete BlockGraph regions and increased failure-family blockers
default action: keep FISSION_ENABLE_MIR_BLOCKGRAPH off
```

### Validation

Passed:

```text
cargo test -p fission-pcode mir_ -- --test-threads=1
cargo test -p fission-pcode blockgraph_region -- --test-threads=1
cargo test -p fission-pcode structuring_admission -- --test-threads=1
cargo test -p fission-pcode suffix_window -- --test-threads=1
cargo test -p fission-pcode structuring_candidate_discovery_ -- --test-threads=1
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
cargo check -p fission-pcode
cargo check -p fission-automation
cargo build -p fission-cli --release
```

Incomplete:

```text
cargo test -p fission-pcode -- --test-threads=1
```

Reason:

- A background cargo/rust-analyzer workspace check repeatedly held the build directory lock.
- Retried after killing stale cargo list/check processes, but the debug rustc process stalled at 0% CPU.
- Targeted tests and release build completed successfully.

### Duplicate Logic Audit

- Builder still only produces CFG/control evidence and telemetry.
- MIR now owns the added BlockGraph proof vocabulary.
- Structuring consumes the env-gated admission decision.
- Printer remains render-only.
- Benchmark changes are telemetry projection only.

### Next Owner

Do not broaden `FISSION_ENABLE_MIR_BLOCKGRAPH` admission further. The next wave should implement actual MIR join/follow proof completion:

1. convert `MustEmitLabelConflict` into typed MIR join-owner subproofs,
2. prove smallest-complete-owner follow selection,
3. isolate/snapshot builder state for failed MIR BlockGraph trials,
4. only then allow graph-collapse output to survive fail-closed fallback.

---

## 4. fission-sleigh x86-64 Front-End Migration: compiler-only wave

### Summary

This wave starts the clean-room Sleigh front-end migration inside `fission-sleigh` with `x86-64` fixed as the first consumer.

Scope was intentionally limited to compiler-only ownership:

- tokenizer
- preprocessor
- parser
- compile-time inventory / pattern graph / semantic action IR
- deterministic generated artifact emission
- non-runtime equivalence harness

The canonical runtime path did not change.

### Why x86-64 first

`x86-64` is the correct first migration target because the canonical validation surface is already the Windows small C corpus:

- `benchmark/config/benchmark_corpus/windows_small_c_samples.json`
- `benchmark/binary/x86-64/window/small/binary/c`

This lets the front-end migration use the same sample family that already drives quality and throughput decisions.

### What changed

New compiler-only owner tree:

- `crates/fission-sleigh/src/compiler/mod.rs`
- `crates/fission-sleigh/src/compiler/token.rs`
- `crates/fission-sleigh/src/compiler/preprocessor.rs`
- `crates/fission-sleigh/src/compiler/ast.rs`
- `crates/fission-sleigh/src/compiler/ir.rs`
- `crates/fission-sleigh/src/compiler/codegen.rs`
- `crates/fission-sleigh/src/compiler/equivalence.rs`

New generated artifact path:

- `crates/fission-sleigh/generated/x86/`

New regeneration entrypoint:

- `cargo run -p fission-sleigh --example generate_x86_frontend`

Generated artifacts now checked in:

- `include_expanded_manifest.json`
- `parsed_inventory.json`
- `normalized_pattern_graph.json`
- `semantic_action_ir.txt`
- `generated_frontend.rs`

### Front-end contract in this wave

The clean-room compiler currently does all of the following for `x86-64.slaspec`:

- resolves the checked-in include graph
- evaluates `@define`, `@ifdef`, `@ifndef`, `@else`, `@endif`
- parses `define`, `macro`, `with`, and constructor blocks into AST
- compiles constructor inventory and pcodeop inventory
- emits deterministic generated artifacts

The wave does **not** yet do:

- runtime decode from compiled tables
- replacement of the hand-written x86 lifter
- CLI behavior change
- benchmark semantic output change

### Equivalence harness

The new equivalence harness is intentionally compiler-only and fail-closed.

Current comparison behavior:

- runs existing hand-lifter decode/lift on sampled x86-64 instruction windows
- records decode length, control-flow class, and emitted pcode opcode sequence
- reports generated-front-end side as:
  - `unsupported_generated_semantic`

This is the correct behavior for this wave because runtime semantic execution from compiled spec tables has not been enabled yet.

Sample sources validated:

- fixed x86 unit seeds
- function-entry windows sampled from:
  - `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe`

### Determinism / checked-in output

New compiler tests now enforce:

- x86 include graph resolution
- conditional preprocessing correctness
- AST discovery of with-blocks and constructors
- compile-time pcodeop / pattern inventory generation
- deterministic artifact rendering
- checked-in generated artifacts exactly matching renderer output

This closes the previous gap where generated output could exist without being verified against the checked-in tree.

### Validation

Passed:

```text
cargo test -p fission-sleigh
cargo run -p fission-sleigh --example generate_x86_frontend
cargo check -p fission-cli
```

Observed result:

```text
fission-sleigh: 307 passed / 0 failed
generated artifact root: crates/fission-sleigh/generated/x86
```

### Artifact shape

Generated artifact sizes in the current snapshot:

```text
include_expanded_manifest.json: 573 bytes
generated_frontend.rs: 24,961 bytes
semantic_action_ir.txt: 432,676 bytes
normalized_pattern_graph.json: 669,507 bytes
parsed_inventory.json: 730,095 bytes
```

This is large but still repo-manageable for a compiler-only artifact set. No runtime path consumes these files yet.

### What improved

- `fission-sleigh` now has a real clean-room compiler spine instead of only hand-written lift code.
- `x86-64` migration has a deterministic, repo-tracked generated output contract.
- front-end validation is now anchored to the same Windows x86-64 sample family used elsewhere in Fission.
- the compiler-only equivalence harness makes the “not integrated yet” state explicit instead of silently implying parity.

### What did not improve

- runtime decode/lift behavior is unchanged
- benchmark quality numbers are unchanged by design
- generated front-end still reports `unsupported_generated_semantic` because execution tables are not wired in this wave

### Duplicate-logic audit

This wave intentionally avoided duplicating runtime semantics.

- existing hand-lifter remains the only executable semantic owner
- compiler-only front-end owns spec parsing / inventory / generated output
- equivalence harness projects differences without pretending to be a second runtime decoder

### Next owner

Do not replace the hand-lifter yet.

The next migration wave should implement x86-64 compiled front-end execution in this order:

1. constructor/table lookup from compiled pattern graph
2. decode-length parity for sampled instruction windows
3. control-flow class parity
4. pcode opcode / varnode-shape parity buckets
5. only then consider execution-path swap behind an env gate

## Sleigh All-Variant Generic Compiler Consumer Expansion

### Summary

Expanded the `fission-sleigh` clean-room compiler-only front-end from the x86-64 first consumer to every checked-in `.slaspec` entry variant using one generic compiler API.

This remains a compiler-only wave:

- no runtime decoder replacement
- no CLI behavior change
- no decompiler benchmark output change expected
- no dependency on `vendor/rsleigh`

### Public/internal contract

New canonical compiler API surface:

- `discover_entry_specs_for_arch(arch)`
- `discover_all_entry_specs()`
- `compile_frontend_for_entry_spec(entry_spec)`
- `compile_frontends_for_arch(arch)`
- `write_generated_artifacts_for_entry_spec(entry_spec, output_root)`
- `write_all_generated_artifacts(output_root)`

Compatibility retained:

- `compile_x86_64_frontend()` remains a wrapper over the generic entry-spec compiler.

### Generated artifact layout

Generated outputs moved to variant-safe paths:

```text
crates/fission-sleigh/generated/<arch>/<entry-spec-stem>/
```

Each variant emits the same five compiler-only artifacts:

- `include_expanded_manifest.json`
- `parsed_inventory.json`
- `normalized_pattern_graph.json`
- `semantic_action_ir.txt`
- `generated_frontend.rs`

The all-variant manifest is now checked in at:

```text
crates/fission-sleigh/generated/compiler_manifest.json
```

Observed manifest result:

```text
variant_count: 48
compile_status ok: 48

aarch64: 3
arm32: 16
mips: 6
powerpc: 18
riscv: 3
x86: 2
```

### Parser / preprocessor fixes required for all variants

The generic compiler needed broader SLEIGH front-end coverage beyond the x86-64 subset:

- `@if` / `@elif` support added.
- Boolean preprocessor expressions now support `defined(NAME)`, `==`, `!=`, `&&`, `||`, and parentheses.
- Comment stripping now respects `"#"` inside strings.
- Braced block collection now ignores braces in comments.
- Constructor parsing now accepts no-brace `unimpl` constructors.

These are language-level SLEIGH parser/compiler improvements, not architecture-specific compilers.

### Validation

Passed:

```text
cargo run -p fission-sleigh --example generate_sleigh_frontends
```

Observed result:

```text
48 variants -> /Users/sjkim1127/Fission/crates/fission-sleigh/generated
```

Additional validation commands passed for this wave:

```text
cargo test -p fission-sleigh
cargo check -p fission-sleigh
cargo check -p fission-cli
```

Observed test result:

```text
fission-sleigh: 314 passed / 0 failed
generated tree checksum stable across repeated generation
```

### What improved

- `fission-sleigh` now has one generic compiler-only consumer for all checked-in architecture variants.
- The previous x86-only generated artifact layout no longer risks collisions across variants.
- The compiler manifest makes unsupported/failing future variants visible instead of silently skipping them.
- Runtime hand-lifters remain untouched, so this is a safe compiler-spine expansion.

### What did not change

- No runtime p-code execution uses generated tables yet.
- No decompiler quality metric is expected to move in this wave.
- `x86` and `AArch64` hand-lifter execution paths remain canonical for runtime behavior.

### Duplicate-logic audit

- The compiler is generic across architectures; no arch-specific compiler implementation was added.
- x86-64 compatibility APIs only delegate to the generic entry-spec API.
- Generated artifacts are compiler products only and do not duplicate runtime semantic ownership.

### Next owner

The next migration wave should stay x86-64-first for runtime execution:

1. compile-table constructor lookup
2. decode-length parity against current hand-lifter
3. control-flow class parity
4. p-code opcode and varnode-shape parity
5. env-gated execution-path trial only after parity buckets are stable

## SLEIGH Runtime Hard-Delete Replacement Wave

### Summary

This wave intentionally removed the old architecture-specific hand-lifter path and
started the new generated/compiler runtime owner.

Fixed rollout decision:

```text
migration_mode: hard_delete
registry_scope: all_skeleton
canonical_runtime_owner: crates/fission-sleigh/src/runtime/
old_lifter_path: removed
rsleigh_dependency: none
```

### What changed

- Removed `crates/fission-sleigh/src/lifter/` and all x86/AArch64 hand-lifter backend modules.
- Added `RuntimeSleighFrontend`, `CompiledRuntimeRegistry`, `DecodeContract`, `DecodeStopReason`, and typed runtime errors under `fission_sleigh::runtime`.
- Updated downstream consumers to import `fission_sleigh::runtime::*` instead of `fission_sleigh::lifter::*`.
- Registered all 48 checked-in `.slaspec` variants through the generated/spec discovery path.
- Marked `x86-64` as `ExecutableCandidate`; all other variants are `RegisteredCompileOnly`.
- Extended generated constructor artifacts with runtime-facing metadata:
  - `pattern_signature`
  - `semantic_template_status`
  - `semantic_action_hash`
  - `semantic_op_count`

### Runtime status

The new runtime is deliberately fail-closed.

- `x86-64` is registered as the first executable candidate, but compiled pattern/action execution is not implemented yet.
- Non-x86-64 variants return `UnsupportedGeneratedSemantic`.
- x86-64 decode/lift currently returns `UnsupportedPcodeTemplate` rather than emitting fake p-code.

This means Rust-SLEIGH decompilation can be degraded until the next runtime execution wave. That is an intentional consequence of hard-deleting the old hand-lifter instead of keeping a fallback shim.

### Validation

Passed:

```text
cargo check -p fission-sleigh
cargo run -p fission-sleigh --example generate_sleigh_frontends
cargo test -p fission-sleigh runtime_
cargo test -p fission-sleigh
```

Observed:

```text
runtime_ targeted tests: 2 passed / 0 failed
fission-sleigh full tests: 25 passed / 0 failed
generated variants: 48
```

Additional validation after downstream/runtime wiring:

```text
cargo check -p fission-cli
cargo check -p fission-decompiler-core
cargo build -p fission-cli --release
cargo check -p fission-automation
CLI info/list smoke on test_functions.exe
CLI decomp smoke on test_functions.exe:fibonacci
```

Observed CLI smoke:

```text
info: passed
list --json: passed
decomp --addr 0x140001470 --json: fail-closed fallback
error bucket: UnsupportedPcodeTemplate
```

### Benchmark readout

Ran limited Windows small C corpus benchmark to make the hard-delete impact explicit:

```text
runner: benchmark/full_benchmark/full_decomp_benchmark.py
manifest: benchmark/config/benchmark_corpus/windows_small_c_samples.json
output: benchmark/artifacts/full_benchmark/windows-small-c-sleigh-runtime-hard-delete-latest
limit: 5
baseline: benchmark/artifacts/full_benchmark/windows-small-c-mir-latest
```

Observed result:

```text
weighted_avg_normalized_similarity: 37.604% -> 0.000%
release_promotion_allowed: false
x64 failed binaries: 6/6
direct_success: 0/5 for each sampled binary
cpu process seconds: 0.032143
effective parallelism: 4.078
```

Promotion blockers:

```text
weighted_avg_normalized_similarity regression
row_fidelity_gate failed for all six Windows small C sample binaries
direct_success changed for at least one binary
arch_summary.x64 weighted similarity regression
```

This is an expected negative benchmark result for a hard-delete wave where the replacement runtime is registered but not executable yet.

### Duplicate-logic audit

- Old x86/AArch64 hand-lifter semantic code was removed instead of duplicated under the generated runtime path.
- CFG block reconstruction moved to the runtime owner and remains shared by downstream tests.
- Compiler/codegen remains architecture-generic; no per-architecture compiler was added.

### Next owner

The next wave must implement executable compiled-table runtime for x86-64:

1. pattern/token table matching
2. operand binding extraction
3. semantic template lowering into `fission_pcode::PcodeOp`
4. differential report against preserved baseline instruction samples
5. Windows small C decomp smoke recovery

## SLEIGH x86-64 Runtime Execution Seed

```text
wave_type: runtime_recovery
runtime_owner: crates/fission-sleigh/src/runtime/
primary_arch: x86-64
old_lifter_path: still removed
rsleigh_dependency: none
external_rust_decode_dependency: iced-x86
promotion_status: not_ready
```

### What changed

- Added an x86-64 executable runtime seed under `crates/fission-sleigh/src/runtime/x86_64.rs`.
- Kept `RuntimeSleighFrontend` / `CompiledRuntimeRegistry` as the canonical runtime owner after the hand-lifter deletion.
- Added decode-only `iced-x86` usage for instruction decoding while keeping p-code emission owned by Fission runtime code.
- Implemented fail-closed typed p-code emission for the initial x86-64 subset:
  - `ret`
  - direct/indirect `call` and `jmp`
  - conditional branches
  - `mov`, `lea`
  - `push`, `pop`, `leave`
  - integer arithmetic/logical ops
  - `cmp`, `test`
  - `movzx`, `movsx`, `movsxd`
  - `setcc`
  - accumulator sign/zero extension family
- Fixed generated artifact JSON escaping for constructor/source strings containing control characters.
- Regenerated all checked-in SLEIGH compiler artifacts.

This is not full compiled-table SLEIGH execution yet. It restores x86-64 runtime execution after the hard-delete wave and establishes the runtime lowering owner. Unsupported generated semantics still return typed errors instead of fake p-code.

### Validation

Passed:

```text
cargo test -p fission-sleigh runtime_
cargo test -p fission-sleigh
cargo check -p fission-sleigh
cargo check -p fission-cli
cargo check -p fission-decompiler-core
cargo check -p fission-automation
cargo build -p fission-cli --release
cargo fmt -p fission-sleigh --check
git diff --check
```

Observed:

```text
runtime_ targeted tests: 5 passed / 0 failed
fission-sleigh full tests: 28 passed / 0 failed
generated artifact JSON parse: passed
generated artifact determinism: passed
```

CLI smoke:

```text
target/release/fission_cli info benchmark/binary/x86-64/window/small/binary/c/test_functions.exe
target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001000 --json
target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001470 --json
```

Observed:

```text
info: passed
decomp 0x140001000: exit 0, engine_used=rust_sleigh, fell_back=false
decomp 0x140001470: exit 0, engine_used=rust_sleigh, fell_back=false
```

### Benchmark readout

Ran limited Windows small C recovery benchmark:

```text
runner: benchmark/full_benchmark/full_decomp_benchmark.py
manifest: benchmark/config/benchmark_corpus/windows_small_c_samples.json
baseline: benchmark/artifacts/full_benchmark/windows-small-c-sleigh-runtime-hard-delete-latest
output: benchmark/artifacts/full_benchmark/windows-small-c-sleigh-x86-runtime-latest
limit: 5
timeout: 120
```

Observed compact summary:

```text
weighted_avg_normalized_similarity: 0.000% -> 44.470%
x64 weighted_avg_normalized_similarity: 44.470%
x64 failed binaries: none
direct_success: 5/5 on each sampled binary
release_promotion_allowed: false
process_cpu_seconds: 0.058375
process_cpu_user_sec: 0.032552
process_cpu_system_sec: 0.025823
process_cpu_utilization_pct: 372.401
effective_parallelism: 3.724
func_per_cpu_second: 516.550167
```

Promotion blockers:

```text
advisory_gate_mode
per-binary row_fidelity_gate failed for all six Windows small C sample binaries
direct_success changed for at least one binary
owner_metric_totals materialization_stabilized: 0.000 -> 198.000
```

Interpretation:

- The hard-delete regression was partially recovered: x86-64 sample decompilation now executes again instead of returning `UnsupportedPcodeTemplate` everywhere.
- This is not a quality promotion result. Row-fidelity still fails and output quality remains below the pre-hard-delete hand-lifter baseline.
- The benchmark improvement is measured against the intentional hard-delete fail-closed baseline, not against the older hand-lifter path.

### Duplicate-logic audit

- No old hand-lifter compatibility shim was reintroduced.
- x86-64 p-code lowering now lives in the new runtime owner, not in CLI/static/printer layers.
- The current runtime seed still duplicates semantic intent that should ultimately come from generated SLEIGH semantic templates. That is accepted only as an executable bridge; the next owner remains compiled-table semantic template lowering.

### Next owner

The next runtime wave must replace the x86-64 bridge with real generated-table execution:

1. generated pattern matcher over compiled constructor tables
2. operand binding extraction from pattern matches
3. semantic template lowering into `fission_pcode::PcodeOp`
4. differential parity harness against preserved x86-64 p-code samples
5. full Windows small C benchmark without `--limit`

## SLEIGH x86-64 Compiled-Table Execution Migration

```text
wave_type: runtime_migration
runtime_owner: crates/fission-sleigh/src/runtime/
primary_arch: x86-64
canonical_reference: vendor/ghidra/ghidra-Ghidra_12.0.4_build
cutover_gate: FISSION_ENABLE_GENERATED_X86_64_RUNTIME
iced_x86_status: temporary bridge_oracle only
promotion_status: not_ready
```

### What changed

This wave moves the x86-64 runtime one owner closer to the Ghidra `SleighLanguage -> DecisionNode -> ConstructState/ParserWalker -> PcodeEmit` spine without deleting the bridge yet.

Compiler-side executable IR was promoted from inventory-only metadata to runtime-consumable state:

- added `CompiledExecutableConstructor`
- added `CompiledDecisionTree`
- added `CompiledOpcodeMatcher`
- added `CompiledOperandSpec`
- added `CompiledSemanticKind`
- added additive manifest fields:
  - `runtime_ready`
  - `decision_node_count`
  - `constructor_template_count`
  - `unsupported_template_count`

Current x86-64 compile step now produces a typed executable subset rather than just reporting constructor inventory:

- exact-byte and row/page matcher buckets
- operand binding specs for:
  - `ModRmRm`
  - `ModRmReg`
  - `OpcodeReg`
  - `Immediate`
  - `Relative`
  - `FixedRegister`
- typed semantic families for the current executable subset:
  - `ret`
  - `call`
  - `jmp`
  - `jcc`
  - `mov`
  - `lea`
  - `push`
  - `pop`
  - `leave`
  - arithmetic/logical core
  - `cmp`
  - `test`
  - `movzx` / `movsx` / `movsxd`
  - `setcc`
  - `cbw` / `cwde` / `cdqe`

Runtime-side owner changes:

- added env-gated generated runtime entrypoint under:
  - `crates/fission-sleigh/src/runtime/generated_x86_64.rs`
- `RuntimeSleighFrontend` now precompiles the executable candidate frontend in-memory
- `decode_and_lift_with_len()` now selects:
  - gate off: existing `iced-x86` bridge path
  - gate on: generated compiled-table matcher/binder/emitter path
- downstream public runtime API remains unchanged

Differential harness was upgraded from front-end smoke to runtime parity classification:

- `decision_tree_no_match`
- `constructor_selection_mismatch`
- `operand_binding_mismatch`
- `semantic_template_unsupported`
- `pcode_opcode_mismatch`
- `varnode_shape_mismatch`
- `branch_target_mismatch`
- `temporary_space_mismatch`

Generated artifacts remain deterministic and repo-tracked. This wave keeps the generated files as debug/diff/readout outputs while runtime consumes the in-memory compiled IR as the canonical owner.

### Validation

Passed:

```text
cargo check -p fission-sleigh
cargo check -p fission-cli
cargo build -p fission-cli --release
cargo test -p fission-sleigh -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_ret -- --nocapture
cargo test -p fission-sleigh runtime::generated_x86_64::tests::generated_runtime_decodes_mov_imm64 -- --nocapture
cargo test -p fission-sleigh generated_runtime_decodes_jcc_rel8 -- --nocapture
cargo test -p fission-sleigh equivalence_report_runs_for_unit_seeds -- --nocapture
```

Observed:

```text
fission-sleigh full tests: 31 passed / 0 failed
generated runtime seed tests: passed
equivalence runtime report test: passed
generated artifact determinism: passed
```

Determinism check:

```text
before=4187a135bd6dd985b5bec98bb48dd1e72ea8667a
after=4187a135bd6dd985b5bec98bb48dd1e72ea8667a
deterministic=1
```

CLI smoke with generated runtime enabled:

```text
FISSION_ENABLE_GENERATED_X86_64_RUNTIME=1 \
target/release/fission_cli decomp \
  benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --addr 0x140001470 --json
```

Observed:

```text
exit=0
engine_used=rust_sleigh
row.name=fibonacci
row.address=0x140001470
preview_build_stats present
```

### Limited benchmark readout

Ran a limited Windows small C corpus benchmark against the previous bridge baseline:

```text
runner: benchmark/full_benchmark/full_decomp_benchmark.py
manifest: benchmark/config/benchmark_corpus/windows_small_c_samples.json
baseline: benchmark/artifacts/full_benchmark/windows-small-c-sleigh-x86-runtime-latest
output: benchmark/artifacts/full_benchmark/windows-small-c-sleigh-compiled-runtime-latest
limit: 5
timeout: 120
env: FISSION_ENABLE_GENERATED_X86_64_RUNTIME=1
```

Observed:

```text
weighted_avg_normalized_similarity: 44.470% -> 35.990%
delta: -8.480pp
promotion_status: failed
per-binary row_fidelity_gate: failed on all 6 sample binaries
crash rows: none observed in the limited corpus run
```

Representative degraded rows from compact summary:

```text
array-operations: WinMainCRTStartup @ 0x1400013e0
- previous_normalized_similarity: 60.26
- current_normalized_similarity: 38.22
- delta: -22.04

bitops-control-flow: WinMainCRTStartup @ 0x1400013e0
- previous_normalized_similarity: 60.26
- current_normalized_similarity: 38.22
- delta: -22.04
```

Interpretation:

- the generated compiled-table path is now executable and crash-free on the limited x64 sample corpus
- this is not yet parity-ready
- the current executable subset is selecting/emitting enough p-code to decompile, but not enough to preserve prior x86 bridge fidelity
- therefore `iced-x86` deletion remains blocked

### Duplicate-logic audit

- Compiler executable IR remains architecture-generic; no per-architecture compiler was introduced.
- The generated runtime path reuses the canonical `RuntimeSleighFrontend` owner instead of adding a CLI/static fallback shim.
- A temporary duplication still exists between:
  - bridge-side handwritten x86 semantic lowering
  - generated runtime typed semantic emitter

This duplication is intentionally temporary and only acceptable while differential parity is still open. It is the next structural debt to remove once compiled semantic template lowering is complete.

### Final status

```text
result: negative_but_useful
hard_delete_gate: still closed
iced_x86_removal: blocked
next_owner: compiled semantic template lowering and constructor-selection parity
```

### Next owner

The next executable runtime wave must focus on parity, not broader instruction coverage:

1. tighten constructor selection against `decision_tree_no_match` and selection drift
2. close operand binding mismatches on startup/runtime-heavy rows first
3. replace handwritten generated-runtime semantic families with compiled semantic template lowering
4. rerun limited corpus until bridge-baseline similarity is non-worse
5. only then open the `iced-x86` deletion gate
