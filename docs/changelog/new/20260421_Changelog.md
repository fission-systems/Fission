# 2026-04-21 Changelog

## Summary

This update focuses on **project maturity and operator-surface cleanup** rather than decompiler semantic changes.

The main goals of this wave were:

- make `benchmark/full_benchmark/` the canonical benchmark surface
- move benchmark configuration ownership under `benchmark/config/`
- align CI/reusable workflows with the new benchmark and artifact roots
- stabilize the human-facing `fission_cli` subcommand surface
- add a detailed CLI reference for evaluators and headless users

No Rust decompiler algorithm behavior was intentionally changed in this wave.

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
