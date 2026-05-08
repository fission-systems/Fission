# 2026-04-21 Changelog

## Summary

This update focuses on **project maturity and operator-surface cleanup** rather than decompiler semantic changes.

The main goals of this wave were:

- make `benchmark/full_benchmark/` the canonical benchmark surface
- move benchmark configuration ownership under `benchmark/config/`
- align CI/reusable workflows with the new benchmark and artifact roots
- stabilize the human-facing `fission_cli` subcommand surface
- add a detailed CLI reference for evaluators and headless users

The initial maturity wave below did not intentionally change Rust decompiler semantics.
The follow-up wrapper-contraction section added later in this changelog does change the explicit rust-sleigh decompilation path for proven wrapper families.

---

## 0. Wrapper Contraction And Explicit Pathological Function Relief

### Scope

This follow-up wave introduced a canonical wrapper-contraction path for zero-size direct tail wrappers and aligned the CLI rust-sleigh execution path to use the same owner.

Primary target:

- `test_functions.exe @ 0x140002d40`
- function name: `register_frame_ctor`
- prior family attribution: `ZeroSizeRuntimeWrapper`

### What changed

- Added typed procedure-summary vocabulary for wrapper contraction in `fission-pcode`.
- Added a narrow direct-tail-wrapper proof for p-code / first-instruction decode.
- Added additive telemetry:
  - `procedure_summary_contracted_count`
  - `procedure_summary_tail_wrapper_count`
  - `procedure_summary_import_thunk_count`
- Added an early wrapper probe in `fission-decompiler-core` that:
  - decodes only the first instruction
  - proves direct tail-wrapper shape
  - emits a minimal synthetic wrapper HIR
  - skips the giant normalize / structuring path entirely when the proof completes
- Removed duplicate rust-sleigh rendering logic from the canonical CLI `decomp` execution path and routed it through `fission-decompiler-core`.

This is the important ownership cleanup in this wave:

- before: `crates/fission-cli/.../decompile_exec/run.rs` had its own local rust-sleigh render path
- after: the canonical `decomp` path consumes the shared `fission-decompiler-core` rust-sleigh path, so wrapper contraction is not bypassed

### Explicit pathological row improvement

Before this wave, the explicit pathological wrapper row was dominated by giant-function cost:

- wall time: roughly `247s`
- `build_duration_ms=247793`
- `normalize_duration_ms=155567`
- `structuring_duration_ms=90686`
- `render_duration_ms=4`
- `replacement_plan_candidate_count=39381`
- `materialization_stabilized_count=33633`

After this wave:

- command:
  - `target/debug/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140002d40 --json`
- wall time:
  - `0.91s`
- emitted code:
  - `undefined register_frame_ctor() { return __gcc_register_frame(); }`
- preview build stats on the explicit row:
  - `procedure_summary_contracted_count=1`
  - `procedure_summary_tail_wrapper_count=1`
  - `procedure_summary_import_thunk_count=0`
  - `replacement_plan_candidate_count=0`
  - `materialization_stabilized_count=0`
  - `normalize_duration_ms=0`
  - `structuring_duration_ms=0`
  - `rendered_code_len=71`

Net effect:

- explicit wrapper wall time dropped by well over `99%`
- giant normalize / materialization / structuring explosion is bypassed for the proven wrapper family

### Whole-binary throughput remained stable

Filtered `--all --benchmark` on the Windows x86-64 small C set remained fast and crash-free:

- `test_functions.exe`
  - `functions_discovered_total=118`
  - `functions_selected_total=78`
  - `functions_excluded_import_count=39`
  - `functions_excluded_runtime_wrapper_count=1`
  - `wall_clock_sec=0.423465`
- `bitops_and_control_flow.exe`
  - `functions_discovered_total=120`
  - `functions_selected_total=80`
  - `functions_excluded_import_count=39`
  - `functions_excluded_runtime_wrapper_count=1`
  - `wall_clock_sec=0.300326`
- `function_pointers_and_strings.exe`
  - `functions_discovered_total=134`
  - `functions_selected_total=91`
  - `functions_excluded_import_count=42`
  - `functions_excluded_runtime_wrapper_count=1`
  - `wall_clock_sec=0.349343`

### Same-axis benchmark result

Validation surface:

- benchmark script:
  - `benchmark/full_benchmark/full_decomp_benchmark.py`
- binary:
  - `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe`
- baseline:
  - detached worktree at `HEAD`
- run shape:
  - `--limit 10`

Baseline:

- `avg_normalized_similarity=44.61`
- `coverage_ratio_pct=100.0`
- `both_success_count=10`
- `fission wall_clock_sec=0.503874`
- `fission total_decomp_sec=0.353872`

Trial:

- `avg_normalized_similarity=46.47`
- `coverage_ratio_pct=100.0`
- `both_success_count=10`
- `fission wall_clock_sec=0.507712`
- `fission total_decomp_sec=0.445924`
- `baseline gate status=passed`
- `comparable_to_baseline=true`

Interpretation:

- quality on the seeded same-axis benchmark was non-worse and improved on normalized similarity
- whole-binary seeded timing stayed effectively flat
- the meaningful speed win in this wave is the explicit pathological wrapper path, not the already-filtered whole-binary path

### Benchmark contract fix discovered during validation

Running a single-binary same-axis benchmark exposed two benchmark-side contract bugs:

1. manifest-less single-binary runs were still injecting the fixed `putty` row-fidelity watchlist
2. dynamic row-watchlist objects were passed into tuple-only row-fidelity code paths during baseline comparison

Both were fixed in `benchmark/full_benchmark/grand_finale_support/benchmark_core.py`:

- manifest-less single-binary runs now default to no fixed watchlist
- row-fidelity targets are normalized before snapshot / baseline-gate evaluation

This was necessary to make the same-axis benchmark result interpretable for `test_functions.exe`.

### Duplicate-logic audit outcome

Duplicate logic reduced in this wave:

- wrapper summary classification is shared through `procedure_summary.rs`
- the canonical CLI rust-sleigh decompile path now routes through `fission-decompiler-core`
- the previous local rust-sleigh render implementation in `decompile_exec/run.rs` no longer bypasses wrapper contraction

Next owner after this wave:

- keep wrapper families under the shared procedure-summary owner
- move next to broader interprocedural wrapper/adaptor summaries only if evidence shows additional zero-cost wrappers beyond the direct-tail family
- do not reopen the previous giant explicit wrapper path unless a non-wrapper pathological family remains after summary contraction

---

## 0.1 Structuring Admission Canonicalization For Windows Small C Samples

### Scope

This follow-up wave targeted the first unresolved sample-user-code owner after wrapper contraction:

- canonical surface:
  - `benchmark/full_benchmark/`
  - `benchmark/binary/x86-64/window/small/binary/c`
- primary owner:
  - `crates/fission-pcode/src/nir/structuring/driver.rs`
- representative row:
  - `test_functions.exe @ 0x140001470`
  - function name: `fibonacci`

The goal in this wave was not to broadly relax structuring policy. The goal was to:

- remove the old architecture-specific blanket force-linear admission
- make force-linear fallback reasons explicit and typed
- expose the fallback owner in `NirBuildStats`
- validate the resulting owner split on the Windows small C corpus without regressing row fidelity

### What changed

The structuring driver now uses one canonical admission helper instead of scattered fallback shortcuts:

- added:
  - `StructuringAdmissionReason`
  - `StructuringAdmissionInput`
  - `decide_structuring_admission(...)`
- admission families are now:
  - `GraphCollapse`
  - `ExplicitForceLinear`
  - `IrreducibleBudget`
  - `ExtremeBudget`
- removed the old x64-specific blanket admission that forced linear structuring for medium-sized 64-bit CFGs based only on block/op count

Additive telemetry was also added to `NirBuildStats`:

- `structuring_force_linear_explicit_count`
- `structuring_force_linear_irreducible_budget_count`
- `structuring_force_linear_extreme_budget_count`

This keeps the policy owner inside `nir/structuring/` and avoids re-encoding fallback attribution in benchmark/reporting layers.

### Validation result

The shipped state in this wave keeps sample-corpus behavior non-worse.

Windows small C 2-way corpus:

- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-structuring-baseline`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-structuring-latest`
- manifest:
  - temporary six-binary Windows small C corpus covering:
    - `test_functions.exe`
    - `bitops_and_control_flow.exe`
    - `function_pointers_and_strings.exe`
    - `structs_and_pointers.exe`
    - `array_operations.exe`
    - `math_operations.exe`

Corpus-level before/after:

- weighted average normalized similarity:
  - `37.602857 -> 37.604286`
- x64 failed binaries:
  - `0 -> 0`
- `materialization_stabilized` total:
  - `15104 -> 15097`
- shape-drift totals:
  - `generic_local_name_sum: 501 -> 493`
  - `goto_total: 402 -> 383`
  - `top_level_label_total: 287 -> 275`

The corpus remained advisory-only, but the important result is:

- `comparable_to_baseline=true`
- no new failed rows
- row-fidelity gates passed on all six binaries

### Representative row readout

`fibonacci @ 0x140001470` remains in the same coarse family, but the fallback owner is now explicit.

Before:

- `decomp_sec=0.235710`
- `build_duration_ms=230`
- `normalize_duration_ms=100`
- `structuring_duration_ms=96`
- `forced_linear_structuring_count=1`
- `region_proof_candidate_count=7`
- `region_proof_completed_count=0`
- `region_emit_ready_failed_count=7`
- `switch_emit_ready_failed_count=7`

After:

- `decomp_sec=0.243272`
- `build_duration_ms=237`
- `normalize_duration_ms=100`
- `structuring_duration_ms=109`
- `forced_linear_structuring_count=1`
- `structuring_force_linear_explicit_count=0`
- `structuring_force_linear_irreducible_budget_count=1`
- `structuring_force_linear_extreme_budget_count=0`
- `region_proof_candidate_count=7`
- `region_proof_completed_count=0`
- `region_emit_ready_failed_count=7`
- `switch_emit_ready_failed_count=7`

Interpretation:

- this row is still not ready for graph-collapse broadening
- the dominant owner is now explicitly narrowed to `IrreducibleBudget`
- the next quality wave should target proof-carrying irreducible/region recovery, not another blanket admission widening

### Negative result that was intentionally not shipped

During validation, a broader irreducible admission trial was tested locally so `fibonacci` could bypass the linear fallback.

That trial was rejected because it caused:

- `row_fidelity_gate failed for 0x140001470`
- `generic_local_name_sum: 311 -> 313` on `test_functions.exe`
- `ReplacementPlanExplosion` on the representative row
- `replacement_plan_candidate_count: 1258 -> 10978`
- `structuring_duration_ms: 96 -> 926`

That broadening is not in the final tree.

The final shipped state stays fail-closed for that family and keeps only:

- canonical owner cleanup
- typed fallback attribution
- additive telemetry

### Duplicate-logic audit outcome

Duplicate logic was reduced in this wave:

- force-linear admission now has one canonical owner in `structuring/driver.rs`
- the old medium-CFG x64 shortcut is no longer duplicated as an implicit architecture policy
- benchmark/reporting now consume typed telemetry instead of reconstructing why a row fell back

Next owner after this wave:

- `Proof-Carrying Region Structuring`
- specifically:
  - irreducible SCC witness refinement
  - guarded-tail rejection narrowing
  - region legality / switch readiness subtype narrowing
- not:
  - another blanket graph-collapse admission broadening

---

## 0.2 BlockGraph / FlowBlock Proof Substrate For Structuring

### Scope

This follow-up wave added a clean-room proof substrate for Ghidra-style `BlockGraph` / `FlowBlock` reasoning inside the Fission structuring owner.

Primary owner:

- `crates/fission-pcode/src/nir/structuring/`

Reference model:

- Ghidra `FlowBlock`, `BlockGraph`, and `ActionStructureTransform` concepts were used as architecture guidance only.
- No Ghidra source code, runtime dependency, generated migration, or binding was introduced.

This wave is diagnostic/refactor-only from a decompiler behavior perspective:

- `wave_type: diagnostic/refactor-only`
- `behavior_changed: no`
- `release_path_changed: no`
- `env_gate: none`

### What changed

The structuring owner now has typed BlockGraph proof vocabulary:

- `BlockGraphRegionKind`
  - `Sequence`
  - `If`
  - `IfElse`
  - `Loop`
  - `Switch`
  - `GuardedTail`
  - `Irreducible`
- `BlockGraphLegalityReason`
  - `Complete`
  - `MissingFollow`
  - `MissingPostdom`
  - `SideEntry`
  - `SideExit`
  - `MustEmitLabelConflict`
  - `AliasInterleave`
  - `EmitReadyIncomplete`
  - `IrreducibleScc`
  - `Budget`
- `BlockGraphRegionProof`
  - entry
  - members
  - exits
  - follow
  - immediate postdominator
  - SCC id
  - legality reason
  - emit-ready flag

The guarded-tail path now records BlockGraph proof evidence through an adapter, but acceptance policy was intentionally kept unchanged. Incomplete proof remains fail-closed, and the printer still does not perform semantic repair.

Additive `NirBuildStats` telemetry:

- `blockgraph_region_candidate_count`
- `blockgraph_region_complete_count`
- `blockgraph_region_rejected_missing_follow_count`
- `blockgraph_region_rejected_must_emit_label_count`
- `blockgraph_region_rejected_emit_ready_count`
- `blockgraph_region_rejected_irreducible_count`

Benchmark/reporting now surfaces these metrics in:

- single-binary verbose JSON/Markdown
- corpus verbose JSON/Markdown
- compact summary JSON
- console summaries
- Python contract tests

The compact summary also now carries targeted structuring rows, including:

- `test_functions.exe:fibonacci @ 0x140001470`
- `math_operations.exe:fibonacci_memo`
- control-heavy degraded rows when available

### Benchmark result

Validation surface:

- benchmark script:
  - `benchmark/full_benchmark/full_decomp_benchmark.py`
- corpus:
  - six Windows x86-64 small C binaries under `benchmark/binary/x86-64/window/small/binary/c`
- baseline artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-ghidra-action-latest`
- trial artifact:
  - `benchmark/artifacts/full_benchmark/windows-small-c-blockgraph-structuring-latest`
- first-pass artifact:
  - `benchmark_compact_summary.json`

Corpus result:

- weighted average normalized similarity:
  - `37.604286 -> 37.604286`
- row gates:
  - passed on all six binaries
- promotion blockers:
  - `advisory_gate_mode`
- new failed rows:
  - none observed in the compact summary

BlockGraph proof totals:

- `blockgraph_region_candidate_count=426`
- `blockgraph_region_complete_count=0`
- `blockgraph_region_rejected_missing_follow_count=0`
- `blockgraph_region_rejected_must_emit_label_count=426`
- `blockgraph_region_rejected_emit_ready_count=0`
- `blockgraph_region_rejected_irreducible_count=0`

Interpretation:

- this wave did not claim a pseudocode quality uplift
- it converted the coarse structuring failure surface into a narrower BlockGraph owner
- the immediate dominant owner is now `MustEmitLabelConflict`, not an unknown generic emit-ready failure

### Representative row readout

`test_functions.exe:fibonacci @ 0x140001470` remained stable:

- `build_duration_ms=245`
- `normalize_duration_ms=103`
- `structuring_duration_ms=111`
- `render_duration_ms=0`
- `rendered_code_len=40935`
- `forced_linear_structuring_count=1`
- `structuring_scc_component_count=13`

This means `fibonacci` still needs a real BlockGraph legality fix. The current wave only made the reason auditable and reportable.

Other corpus totals:

- `NormalizeHeavy=34`
- `StructuringHeavy=1`
- `materialization_stabilized=15097`
- `generic_local_name_sum=493`
- `generic_param_name_sum=218`
- `goto_total=383`
- `top_level_label_total=275`
- `synthetic_helper_call_total=33`

### Validation

Passed:

```text
cargo fmt --all
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
cargo test -p fission-pcode blockgraph_region -- --test-threads=1
cargo test -p fission-pcode ghidra_action -- --test-threads=1
cargo check -p fission-pcode
cargo check -p fission-automation
cargo build -p fission-cli
```

Known residual failures:

```text
cargo test -p fission-pcode suffix_window -- --test-threads=1
result: 56 passed, 7 failed

cargo test -p fission-pcode structuring_candidate_discovery_ -- --test-threads=1
result: 33 passed, 18 failed

cargo test -p fission-pcode -- --test-threads=1
result: 639 passed, 25 failed
```

The residual failure family is still guarded-tail / suffix-window structuring. This wave did not resolve the `25 -> 0` target; it made the next owner concrete.

### Duplicate-logic audit outcome

Duplicate semantic repair was not added.

- builder remains limited to producing control evidence
- structuring owns region legality
- printer remains a renderer and label/goto cleanup layer only
- benchmark/reporting only projects telemetry

Next owner after this wave:

- reduce `blockgraph_region_rejected_must_emit_label_count=426`
- migrate guarded-tail / suffix-window legality into the BlockGraph proof path until the current 25 residual tests are fixed
- only after that, consider a narrow behavior-changing acceptance rule for `fibonacci`

---

## 1. Benchmark Surface Canonicalization

### Canonical benchmark entrypoint

The canonical benchmark entrypoint is now documented and treated as:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py ...
```

This replaces older documentation and workflow references that still pointed at older benchmark script roots.

### Canonical benchmark roots

Benchmark ownership is now documented around these roots:

- `benchmark/full_benchmark/`
- `benchmark/config/benchmark_corpus/`
- `benchmark/artifacts/full_benchmark/`
- `benchmark/artifacts/automation/`

### Documentation updates

The benchmark/operator documentation was rewritten to reflect the current source of truth:

- `benchmark/full_benchmark/README.md`
- `benchmark/BENCHMARK_GUIDE.md`
- `benchmark/IMPLEMENTATION_SUMMARY.md`
- `README.md`

These docs now consistently describe:

- Windows-only corpus focus for the current wave
- advisory-first corpus benchmark semantics
- compact summary JSON as the preferred first-pass machine-readable artifact
- `putty` as a primary canary, but not the only benchmark narrative

### Legacy path cleanup in docs

The documentation cleanup removed or replaced stale references such as:

- `artifacts/batch_benchmark_scripts/...`
- `config/benchmark_corpus/...`
- older benchmark result/history ownership language

---

## 2. Benchmark Config Ownership And Validation

### Config ownership moved to `benchmark/config/`

Benchmark configuration ownership is now explicitly centered under:

- `benchmark/config/benchmark_corpus/*.json`
- `benchmark/config/automation/sentinel_sets.toml`

This makes benchmark configuration easier to reason about and separates it from older repo-root conventions.

### Corpus manifest validation

The Python benchmark contract tests were extended so checked-in manifests must satisfy the current suite contract.

The updated validation covers:

- required top-level metadata
- valid `suite_tier`
- valid `gate_mode`
- `dynamic_watchlist_limit`
- Windows sample path scope
- x86/x64 derivation viability
- default artifact naming contract

Primary test file:

- `benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py`

---

## 3. CI / Reusable Workflow Alignment

### Workflow path updates

Reusable workflows were updated to match the new benchmark/config/artifact ownership model.

Updated areas include:

- benchmark workflow rooting
- corpus validation workflow rooting
- automation artifact upload path
- benchmark job summary fields

Files updated:

- `.github/workflows/_reusable/benchmark.yml`
- `.github/workflows/_reusable/corpus-validation.yml`
- `.github/workflows/_reusable/nir-check.yml`
- `.github/CI_CD_GUIDE.md`

### CI behavior intent

The workflow model now more clearly separates:

- fast CI responsibilities
- heavy validation responsibilities
- advisory benchmark reporting
- promotion eligibility reporting

Benchmark job summaries now explicitly surface:

- benchmark status
- gate mode
- release promotion eligibility
- x86/x64 summary visibility
- promotion blockers when available

---

## 4. Benchmark Helper Script Cleanup

The operator/helper scripts were updated to stop referencing stale benchmark roots or stale CLI entry shapes.

Files updated:

- `benchmark/full_benchmark/validate_limit_regression.py`
- `benchmark/full_benchmark/find_timeout_culprit.py`

These updates primarily:

- point to the canonical benchmark entrypoint
- use the canonical CLI subcommand surface
- reduce confusion during local repro and debugging loops

---

## 5. CLI Surface Reorganization Follow-Through

### Canonical subcommands

The CLI now consistently presents the explicit subcommand model as the canonical public surface:

- `info`
- `list`
- `disasm`
- `decomp`
- `strings`
- `inventory`

The goal is to keep human-facing one-shot usage separate from operator-grade inventory and batch emitters.

### Legacy flat invocation policy

Legacy flat invocations still exist as deprecated compatibility shims, but the messaging now makes the intended transition path explicit:

- legacy flat syntax remains functional
- it emits deprecation warnings
- it normalizes into the canonical subcommand execution path

This preserves compatibility without preserving two user-facing command models.

### Help/usage polish

The CLI help surface was improved with:

- clearer top-level positioning
- explicit human-facing vs operator-facing distinction
- subcommand-specific long descriptions
- copy-pasteable examples per subcommand
- updated legacy compatibility wording

Files updated:

- `crates/fission-cli/src/cli/args.rs`
- `crates/fission-cli/src/cli/oneshot/mod.rs`

---

## 6. New Detailed CLI Documentation

### Added user-facing CLI reference

A new detailed CLI reference was added:

- `docs/CLI.md`

This document now serves as the detailed user/operator guide for `fission_cli`.

### Scope of the new CLI guide

The new guide documents:

- canonical command model
- build and invocation basics
- `info`, `list`, `disasm`, `decomp`, `strings`, and `inventory`
- `decomp` option ownership and meaning
- JSON vs text usage guidance
- operator-grade `inventory` workflows
- legacy compatibility behavior
- benchmark boundary rules
- recommended validation commands

### README integration

`README.md` now links directly to `docs/CLI.md` so evaluators can move from quick start to detailed command reference without reading contributor-only material.

---

## 7. AGENTS / Ownership Documentation Updates

Contributor/ownership guides were updated to match the current repo layout and benchmark/CLI ownership model.

Files involved:

- `AGENTS.md`
- `crates/fission-automation/AGENTS.md`
- `crates/fission-cli/AGENTS.md`

These updates reinforce:

- benchmark runner ownership under `benchmark/full_benchmark/`
- benchmark config ownership under `benchmark/config/`
- CLI subcommand ownership under `crates/fission-cli/`
- separation between semantic repair layers and reporting/UI layers

---

## 8. External Headless Evaluation Pack

### New external evaluation guide

A new evaluator-facing guide was added:

- `docs/EVALUATION.md`

This document is intended for external teams evaluating Fission from the CLI in a headless workflow.

It explicitly covers:

- current best-supported evaluation scope
- Windows x64-first evaluation guidance
- checked-in sample binaries
- 30-minute first-pass CLI workflow
- deeper inventory-oriented evaluation
- benchmark as a second-stage activity rather than the first entrypoint
- capability boundaries around CLI, Rust crate use, Sleigh, and Python bindings

### Checked-in example outputs

Small checked-in CLI example artifacts were added under:

- `docs/examples/cli/`

These examples show the expected shape of:

- `info`
- `list --json`
- `decomp --addr`
- `decomp --addr --json`
- `inventory function-facts` summary JSON

The purpose is to reduce evaluator uncertainty and make the CLI/output contract visible before running the tool.

### README / CLI routing updates

The public-facing docs were tightened so evaluators land on the right entrypoint faster.

Files updated:

- `README.md`
- `docs/CLI.md`

These updates now:

- route external evaluators to `docs/EVALUATION.md`
- keep `docs/CLI.md` as the detailed command reference
- position the CLI as the primary documented product surface
- separate manual CLI evaluation from benchmark workflows

---

## 9. Speed Bottleneck Wave 1: Batch Target Filtering

### Goal

This wave targeted whole-binary CLI/operator throughput rather than NIR/core pass internals.

The focus was to stop sending obvious non-user functions through the full batch decompilation path by default.

### New default batch-selection policy

Whole-binary batch selection now filters the following by default:

- imported functions (`is_import == true`)
- zero-size runtime wrapper:
  - `register_frame_ctor`

This behavior applies to multi-function/operator surfaces such as:

- `fission_cli decomp <binary> --all`
- `fission_cli inventory function-facts ...`
- shared batch/inventory selector paths used by preview-candidate workflows

It does **not** change exact-address behavior for:

- `--addr`
- `--addresses-file`
- `list`
- `info`

### New compatibility flag

A new public CLI flag was added:

- `--include-nonuser-functions`

This restores the old whole-function-set behavior for batch/operator workflows when compatibility or forensics coverage is intentionally desired.

### Shared selector/accounting

The batch target policy is now centralized in the shared function-selection layer instead of being duplicated per command.

This also introduced stable selection accounting fields such as:

- `functions_discovered_total`
- `functions_selected_total`
- `functions_excluded_import_count`
- `functions_excluded_runtime_wrapper_count`
- `include_nonuser_functions`

These counts now surface in:

- `decomp --all --json --benchmark` metadata
- inventory summary JSON
- preview-candidate batch/inventory summary shapes where the selector contract is reused

### Validation result

The speed gain came from target-set reduction, not from any pass-level optimization.

Measured whole-binary comparisons on the Windows x86-64 small C corpus showed:

- `test_functions.exe`
  - `258.55s -> 0.36s`
  - selected `78 / 118`
  - excluded imports `39`
  - excluded runtime wrappers `1`
- `bitops_and_control_flow.exe`
  - `191.21s -> 0.25s`
  - selected `80 / 120`
  - excluded imports `39`
  - excluded runtime wrappers `1`
- `function_pointers_and_strings.exe`
  - `245.04s -> 0.31s`
  - selected `91 / 134`
  - excluded imports `42`
  - excluded runtime wrappers `1`

These runs remained crash-free.

### Important boundary

This wave did **not** tune:

- `wide_dead_assignment`
- `sccp`
- `jump_resolver`
- `break_continue_recovery`

It is a throughput admission/filtering change only, not a semantic optimization wave.

---

## 10. Speed Bottleneck Wave 2: `wide_dead_assignment` Rerun Admission Trial

### Goal

This wave targeted the remaining normalize-time bottleneck after batch filtering by narrowing only the **rerun** portion of `wide_dead_assignment`.

The first `defuse_dead_assignment_pass` remains mandatory. Only reruns 2..6 are now gated.

### Env gate

This trial is protected by a new default-off env gate:

- `FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION`

When the gate is enabled:

- first pass always runs
- reruns are admitted only if:
  - `count_hir_stmts(body) <= 220`
  - `locals.len() <= 160`
- otherwise reruns are skipped and the first-pass result is kept

### New telemetry and reporting

Trial-specific telemetry was added to `NirBuildStats`:

- `wide_dead_assignment_rerun_admitted_count`
- `wide_dead_assignment_rerun_skipped_by_admission_count`

Benchmark/reporting now also surfaces selected normalize pass metrics for:

- `wide_dead_assignment`
- `sccp`
- `jump_resolver`
- `break_continue_recovery`

These metrics now appear in:

- benchmark verbose JSON/Markdown
- compact summary JSON
- console summary

### Validation result

This wave is **not promotion-ready**.

Observed results:

- Windows small C corpus `limit50` same-axis benchmark:
  - quality stayed neutral
  - weighted corpus similarity stayed unchanged at `37.400%`
  - no new failed rows appeared
- whole-binary filtered `decomp --all --json` runs:
  - `test_functions.exe`: `0.40s -> 0.34s`
  - `bitops_and_control_flow.exe`: `0.24s -> 0.23s`
  - `function_pointers_and_strings.exe`: `0.29s -> 0.29s`
  - selected row counts stayed unchanged
- explicit pathological function check:
  - `register_frame_ctor @ 0x140002d40`
  - `252.25s -> 2164.06s`
  - `wide_dead_assignment` pass time itself decreased slightly, but wall time regressed severely

### Final status

```text
status: default-off negative env-gated result
env_gate: FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION
release_path_changed: no
release promotion: no
reason: quality stayed neutral, but targeted speed objective failed and explicit pathological function latency regressed badly
```

### Practical takeaway

This trial shows that `wide_dead_assignment` rerun admission alone is not a safe next promotion candidate.

The pass-level reporting added here remains useful, but the optimization result itself should stay default-off and be treated as a negative env-gated result.

---

## 8. What This Wave Explicitly Did Not Change

This wave did **not** intentionally change:

- Rust decompiler semantics
- NIR/HIR structuring logic
- release-path decompiler behavior
- benchmark advisory/blocking semantics
- inventory output schemas as part of semantic work

The focus was operational maturity, documentation clarity, and evaluation-readiness.

---

## 9. Practical Outcome

After this wave:

- benchmark ownership is easier to find
- CI references the current benchmark/config roots
- `fission_cli` presents a cleaner headless-first surface
- evaluators have a proper CLI reference instead of only README snippets
- legacy CLI usage is still possible, but the intended migration path is now explicit

This prepares the repo for the next phase of work, where decompilation-quality improvements can be evaluated against a more stable benchmark and operator surface.

---

## 11. Speed Bottleneck Wave 3: Giant Explicit Function Cost Decomposition

### Goal

This wave stayed **diagnostic-only** and targeted the remaining pathological speed owner after the batch filtering work.

The observed bottleneck was no longer filtered whole-binary `--all` throughput. It was the explicit giant-function path, especially:

- `test_functions.exe @ 0x140002d40`
- `register_frame_ctor`

The objective in this wave was to separate:

- normalize cost
- structuring cost
- render cost
- pathological replacement/materialization pressure

without changing decompiler behavior.

### New canonical telemetry

`NirBuildStats` now records additional stage-level telemetry:

- `structuring_duration_ms`
- `render_duration_ms`
- `rendered_code_len`
- `max_structuring_scc_component_size`

These were added additively. Existing fields such as:

- `build_duration_ms`
- `normalize_duration_ms`
- `pass_metrics`
- replacement/materialization counters

retain their existing semantics.

### New benchmark/reporting readout

The benchmark/reporting layer now derives giant-function speed families from the raw telemetry.

New derived families:

- `ZeroSizeRuntimeWrapper`
- `NormalizeHeavy`
- `StructuringHeavy`
- `RenderHeavy`
- `ReplacementPlanExplosion`
- `MixedGiantFunction`
- `UnknownGiantFunction`

Per-binary and corpus summaries now surface:

- `giant_function_candidates`
- `giant_function_speed_family_counts`
- `giant_function_speed_family_totals`
- `max_rendered_code_len`
- `max_structuring_scc_component_count`
- `max_replacement_plan_candidate_count`
- `max_materialization_stabilized_count`
- capped `max_pathological_examples`

This readout is available in:

- verbose JSON/Markdown
- compact summary JSON
- console summary

### Live diagnostic result

The filtered whole-binary path remained stable and fast:

- `test_functions.exe`: `78` selected rows, crash-free
- `bitops_and_control_flow.exe`: `80` selected rows, crash-free
- `function_pointers_and_strings.exe`: `91` selected rows, crash-free

The explicit pathological target remained crash-free and now carries full owner telemetry:

```text
binary: test_functions.exe
addr: 0x140002d40
name: register_frame_ctor
size: 0
build_duration_ms: 247793
normalize_duration_ms: 155567
structuring_duration_ms: 90686
render_duration_ms: 4
rendered_code_len: 452822
structuring_scc_component_count: 228
max_structuring_scc_component_size: 17
replacement_plan_candidate_count: 39381
materialization_stabilized_count: 33633
giant_function_speed_family: ZeroSizeRuntimeWrapper
```

### Practical conclusion

This wave closed a key ambiguity:

- the pathological explicit-function cost is **not** render-dominant in wall time
- normalize remains the largest stage
- structuring is also materially large
- replacement/materialization pressure is extreme
- the current pathological family is now narrowly attributable as:
  - `ZeroSizeRuntimeWrapper`

This is enough evidence to avoid guessing at the next policy wave. The next speed step should target the explicit pathological family directly, not reopen general filtered `--all` throughput work.

### Final status

```text
wave_type: diagnostic-only
behavior_changed: no
release_path_changed: no
env_gate: none
promotion impact: none
```

## Ghidra Clean-Room Action Pipeline Telemetry

### Summary

This wave starts the Ghidra 1:1 clean-room migration spine without changing decompiler behavior. The goal is to make Fission's Rust-native per-function path observable through Ghidra-style owner boundaries:

```text
FuncdataBuild
HeritageValueRecovery
Normalize
PrototypeTypes
BlockGraphStructuring
PrintC
```

This is a clean-room conceptual mapping only. No Ghidra runtime dependency, code copy, or release-path behavior change was introduced.

### Implementation

Added a new internal action pipeline vocabulary under `fission-pcode`:

- `GhidraActionConcept`
- `GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE`
- `record_ghidra_action_stage(...)`
- `record_ghidra_clean_room_pipeline_complete(...)`

Added additive `NirBuildStats` counters:

- `ghidra_action_stage_count`
- `ghidra_action_funcdata_build_count`
- `ghidra_action_heritage_value_recovery_count`
- `ghidra_action_normalize_count`
- `ghidra_action_prototype_types_count`
- `ghidra_action_blockgraph_structuring_count`
- `ghidra_action_printc_count`
- `ghidra_clean_room_pipeline_complete_count`

The benchmark/reporting pipeline now surfaces the same counters in:

- single-binary verbose JSON/Markdown
- corpus verbose JSON/Markdown
- compact summary JSON
- console summaries
- Python contract tests

### Benchmark result

Benchmark command used the Windows small C parity corpus:

```text
benchmark/full_benchmark/full_decomp_benchmark.py
samples: benchmark/binary/x86-64/window/small/binary/c
artifact: benchmark/artifacts/full_benchmark/windows-small-c-ghidra-action-latest
baseline: benchmark/artifacts/full_benchmark/windows-small-c-structuring-baseline
```

Quality remained neutral:

```text
weighted_avg_normalized_similarity: 37.602857 -> 37.604286
x64 weighted_avg_normalized_similarity: 37.604
row gates: passed for all 6 binaries
failed binaries: none
new failed rows: none observed in compact/corpus summary
```

Per-binary readout:

```text
test-functions: 37.280, passed
bitops-control-flow: 36.740, passed
function-pointers-strings: 39.790, passed
structs-pointers: 38.260, passed
array-operations: 36.740, passed
math-operations: 37.140, passed
```

New Ghidra action totals:

```text
stage_count: 1678
funcdata_build: 294
heritage_value_recovery: 294
normalize: 294
prototype_types: 294
blockgraph_structuring: 208
printc: 294
pipeline_complete: 294
```

Owner/shape totals:

```text
materialization_stabilized: 15104 -> 15097
generic_local_name_sum: 501 -> 493
generic_param_name_sum: 218 -> 218
goto_total: 402 -> 383
top_level_label_total: 287 -> 275
synthetic_helper_call_total: 33 -> 33
```

Giant-function family totals:

```text
NormalizeHeavy: 36 -> 34
StructuringHeavy: 0 -> 1
```

Advisory promotion remains blocked, as expected for this diagnostic/refactor wave:

```text
gate_mode: advisory
release_promotion_allowed: false
promotion_blockers:
- advisory_gate_mode
- failure_family_distribution canonical_must_emit_label_conflict_count: 1076 -> 1112
- failure_family_distribution canonical_emit_ready_failed_count: 1016 -> 1034
```

The failure-family blockers confirm the next quality owner is still structuring/region legality, not the action telemetry itself.

### Validation

Passed:

```text
cargo fmt --all
python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py benchmark/full_benchmark/grand_finale_support/test_llm_advisory.py
cargo test -p fission-pcode ghidra_action -- --test-threads=1
cargo check -p fission-pcode
cargo check -p fission-automation
cargo build -p fission-cli
```

Known residual failure:

```text
cargo test -p fission-pcode -- --test-threads=1
result: 637 passed, 25 failed
failure owner: guarded-tail / suffix-window structuring tests
```

This failure group predates the action telemetry wave and remains the next structuring owner to fix. The new Ghidra action tests pass independently.

### Final status

```text
wave_type: diagnostic/refactor-only
primary_owner: Ghidra clean-room action pipeline telemetry
behavior_changed: no
release_path_changed: no
env_gate: none
duplicate semantic logic changed: no semantic repair added outside canonical owners
next owner: BlockGraph/FlowBlock-style proof-carrying structuring, especially guarded-tail and region legality
```

## Ghidra BlockGraph Must-Emit-Label Ownership And Guarded-Tail Canonicalization

### Summary

This wave moved the next Windows small C quality owner forward in two ways:

- the residual guarded-tail / suffix-window crate failure family was fixed
- `MustEmitLabelConflict` is no longer a single opaque benchmark bucket; it now projects subtype totals through the canonical structuring owner and the benchmark layer

This is a behavior-changing quality wave, but it is still not promotion-ready. The sample corpus stayed quality-neutral while a new advisory blocker appeared in the failure-family distribution.

### What changed

Canonical owner stayed inside:

- `crates/fission-pcode/src/nir/structuring/guarded_tail/`
- `crates/fission-pcode/src/nir/types.rs`

Behavioral guarded-tail / suffix-window fixes:

- canonicalized forward-alias chains can now resolve a unique forward target even when the next label sits outside the currently canonicalized slice
- suffix-window rejection precedence now prefers proof-first reasons:
  - nested or nonlocal tail references
  - side-effectful suffix payloads
  - nonterminal goto targets
  - unresolved alias redirects
- candidate external-entry classification now skips the anchor statement itself, which avoids counting the candidate's own branch as an external entry
- paired nested-boundary internalization is no longer double-counted when same-guard-family nested conditional entries already internalize the same shape

Additive BlockGraph subtype telemetry was added to `NirBuildStats`:

- `blockgraph_region_rejected_middle_ref_count`
- `blockgraph_region_rejected_external_ref_count`
- `blockgraph_region_rejected_join_owner_conflict_count`
- `blockgraph_region_rejected_nonterminal_join_count`
- `blockgraph_region_rejected_follow_owner_conflict_count`

Benchmark/reporting now projects the same subtype totals through:

- `benchmark_summary.json`
- `benchmark_compact_summary.json`
- corpus Markdown
- console summaries

This keeps the semantic owner in `fission-pcode`; benchmark code only reads and renders the counters.

### Duplicate-logic audit

Duplicate semantic logic was reduced in this wave:

- suffix-window legality and alias-chain canonicalization now resolve through the shared guarded-tail owner instead of separate overlapping ad hoc checks
- BlockGraph subtype counters are defined once in `NirBuildStats` and projected outward, rather than re-derived in Python
- benchmark/reporting remains telemetry-only and does not perform semantic repair

What did not change:

- printer/layout code still does not own structuring legality
- representative/materialization policy was not broadened here

### Validation

Passed:

```text
cargo test -p fission-pcode suffix_window -- --test-threads=1
result: 63 passed, 0 failed

cargo test -p fission-pcode structuring_candidate_discovery_ -- --test-threads=1
result: 51 passed, 0 failed

cargo test -p fission-pcode -- --test-threads=1
result: 664 passed, 0 failed

python3 -m unittest benchmark/full_benchmark/grand_finale_support/test_corpus_benchmark.py
result: 22 passed

cargo check -p fission-pcode
cargo check -p fission-automation
cargo build -p fission-cli
cargo build -p fission-cli --release
```

Net crate-level improvement:

```text
full-suite residual failures: 25 -> 0
primary fixed family: guarded-tail / suffix-window / structuring candidate discovery
```

### Windows small C 2-way benchmark

Benchmark contract used:

```text
runner:
- benchmark/full_benchmark/full_decomp_benchmark.py

manifest:
- benchmark/config/benchmark_corpus/windows_small_c_samples.json

baseline:
- benchmark/artifacts/full_benchmark/windows-small-c-blockgraph-structuring-latest

trial:
- benchmark/artifacts/full_benchmark/windows-small-c-guarded-tail-ownership-latest

first-pass artifact:
- benchmark/artifacts/full_benchmark/windows-small-c-guarded-tail-ownership-latest/benchmark_compact_summary.json
```

Corpus quality result:

```text
weighted_avg_normalized_similarity: 37.604286 -> 37.604286
x64 weighted_avg_normalized_similarity: 37.604 -> 37.604
coverage_non_worse_count: 6 -> 6
direct_success_non_worse_count: 6 -> 6
new failed rows: 0
top degraded rows: none
row gates: passed for all 6 binaries
```

Owner / shape totals stayed flat:

```text
materialization_stabilized: 15097 -> 15097
generic_local_name_sum: 493 -> 493
generic_param_name_sum: 218 -> 218
goto_total: 383 -> 383
top_level_label_total: 275 -> 275
synthetic_helper_call_total: 33 -> 33
```

BlockGraph proof totals became narrower:

```text
candidate: 426 -> 414
complete: 0 -> 0
rejected_must_emit_label: 426 -> 414
rejected_external_ref: 0 -> 108
rejected_join_owner_conflict: 0 -> 128
rejected_middle_ref: 0 -> 24
rejected_nonterminal_join: 0 -> 0
rejected_follow_owner_conflict: 0 -> 0
```

Representative target rows:

```text
test_functions.exe:fibonacci @ 0x140001470
- normalized_similarity: 11.65 -> 11.65
- forced_linear_structuring_count: 1 -> 1
- region_proof_candidate_count: 7 -> 7
- region_proof_completed_count: 0 -> 0
- region_emit_ready_failed_count: 7 -> 7
- switch_emit_ready_failed_count: 7 -> 7

math_operations.exe:fibonacci_memo @ 0x140001a90
- blockgraph rejected_must_emit_label: 2

function_pointers_and_strings.exe:compare_int_descending @ 0x140001470
- unchanged targeted surface
```

### What improved

Concrete improvements in this wave:

- guarded-tail / suffix-window canonicalization and candidate-discovery regressions are gone at the crate-test level
- `MustEmitLabelConflict` is no longer one opaque bucket in benchmark artifacts
- the sample corpus remains stable with no new failed rows
- `blockgraph_region_candidate_count` and `rejected_must_emit_label` both dropped:
  - `426 -> 414`

### What regressed

The wave is still not promotion-ready.

New advisory blocker:

```text
failure_family_distribution canonical_alias_interleave_conflict_count: 38 -> 50
```

This means the current guarded-tail canonicalization tightened one owner family but shifted pressure into alias-interleave rejection elsewhere. That is a real semantic owner signal, not a benchmark artifact issue.

### Practical conclusion

This wave closed the old residual test gap but did not yet deliver visible sample-corpus pseudocode quality uplift.

The current state is:

- test-family health improved materially
- corpus similarity is neutral
- `fibonacci` is still linearized
- the next owner is narrower than before:
  - `alias_interleave_conflict` inside the guarded-tail / BlockGraph ownership path

### Final status

```text
wave_type: behavior-changing quality wave
primary_owner: BlockGraph must-emit-label ownership + guarded-tail canonicalization
behavior_changed: yes
release_path_changed: no
env_gate: none
promotion impact: blocked
next owner: canonical alias-interleave conflict reduction before any broader structuring acceptance
```

---

## 0.4 Alias-Interleave Owner Narrowing In Benchmark / Compact Artifacts

### Scope

This follow-up wave did not broaden guarded-tail acceptance. It narrowed the next quality blocker into explicit benchmark-visible subtype metrics.

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

## 0.5 Windows Small C Quality Wave: nested-before alias ownership proof

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
