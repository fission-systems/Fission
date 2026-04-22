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
