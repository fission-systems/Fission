# Contributor field guide and playbooks

Moved out of `README.md` when that file was reduced to an orientation
document for people arriving at the repository. Nothing here changed; this is
the long-form contributor material — field guide, per-area playbooks, the
review question bank, and the maintainer handoff template.

Start with [`AGENTS.md`](../../AGENTS.md) for the working rules and the
decompiler quality loop. This document is the depth behind them.

## Field Guide: Practical Rules by Area

### 01. P-code parity

Verify instruction semantics before diagnosing decompiler output.

- Owner check: identify the crate that owns p-code parity before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 02. NIR materialization

Preserve source behavior even when output is temporarily verbose.

- Owner check: identify the crate that owns nir materialization before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 03. HIR cleanup

Improve readability only after the semantic basis is correct.

- Owner check: identify the crate that owns hir cleanup before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 04. Type hints

Promote evidence-backed types and keep provenance inspectable.

- Owner check: identify the crate that owns type hints before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 05. Stack recovery

Model stack slots consistently across calls, locals, and spills.

- Owner check: identify the crate that owns stack recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 06. Pointer recovery

Prefer data-flow-backed pointer reasoning over text substitution.

- Owner check: identify the crate that owns pointer recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 07. Array recovery

Recover indexed forms when stride and base evidence are present.

- Owner check: identify the crate that owns array recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 08. Struct recovery

Recover field access only when layout evidence supports it.

- Owner check: identify the crate that owns struct recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 09. Calling convention

Derive parameters and returns from ABI facts and observed uses.

- Owner check: identify the crate that owns calling convention before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 10. Loop structuring

Use SCC and dominance facts before emitting structured loops.

- Owner check: identify the crate that owns loop structuring before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 11. Switch recovery

Use jump-table evidence and bounds before emitting switch syntax.

- Owner check: identify the crate that owns switch recovery before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 12. Goto fallback

Use explicit fallback when legal structure is not proven.

- Owner check: identify the crate that owns goto fallback before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 13. Printer formatting

Render the model; do not create semantic facts.

- Owner check: identify the crate that owns printer formatting before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 14. Loader identity

Attach evidence without changing parse semantics.

- Owner check: identify the crate that owns loader identity before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 15. Resource lookup

Route through resource roots and path config.

- Owner check: identify the crate that owns resource lookup before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 16. Automation reports

Project canonical metrics and keep outputs deterministic.

- Owner check: identify the crate that owns automation reports before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 17. CI gates

Prefer focused reusable jobs with explicit inputs.

- Owner check: identify the crate that owns ci gates before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 18. Release tags

Tag only after successful push CI for the exact commit.

- Owner check: identify the crate that owns release tags before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 19. Vendor reference

Consult for invariants without copying or depending on code.

- Owner check: identify the crate that owns vendor reference before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

### 20. Documentation

Keep claims grounded in current source and docs.

- Owner check: identify the crate that owns documentation before editing.
- Evidence check: record the concrete function, row, sample, test, or artifact that motivated the change.
- Coverage check: add focused coverage for the invariant rather than only inspecting output manually.
- Regression check: run the smallest useful test first and then the relevant crate check.
- Reporting check: state whether the change is semantic, presentational, telemetry-only, or documentation-only.

## Contributor Playbooks

This section expands the repository rules into concrete scenarios. It is intentionally operational: each playbook describes where to start, what to avoid, and what evidence should exist before claiming the work is done.

### 01. Fixing a raw p-code mismatch

Start in `fission-sleigh`; compare emitted p-code before touching NIR or HIR.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 02. Fixing an NIR materialization bug

Start in `fission-pcode/src/nir`; preserve exact source behavior before readability work.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 03. Improving HIR readability

Start from correct NIR; prefer cleanup passes over printer substitutions.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 04. Changing structuring logic

Start in `fission-pcode/src/nir/structuring`; require CFG evidence and proof completeness.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 05. Adding a new NIR pass

Implement a pass with declared analysis dependencies and accurate changed status.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 06. Debugging an if-else recovery failure

Inspect dominance, post-dominance, and region exits before modifying emitted syntax.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 07. Debugging a loop recovery failure

Inspect SCCs, loop headers, latches, exits, and break or continue candidates.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 08. Debugging a switch recovery failure

Inspect jump table evidence, bounds, case targets, and default target handling.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 09. Cleaning unnecessary temporaries

Confirm the temporary has no semantic or ordering role before removing it.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 10. Recovering function parameters

Use ABI facts, call uses, stack/register evidence, and type context together.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 11. Recovering local variables

Tie stack slots, stores, loads, and lifetimes to stable local names.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 12. Recovering return values

Check accumulator registers, call sites, and observed return uses.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 13. Recovering pointer arithmetic

Prefer base plus offset or indexed forms only when data-flow evidence supports them.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 14. Recovering arrays

Require stable stride, base object, and index expression evidence.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 15. Recovering struct fields

Require layout evidence before rendering field access.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 16. Changing loader format detection

Fail closed for unknown or unsupported families; never hide uncertainty as raw bytes.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 17. Adding loader provenance

Attach evidence without changing parsing semantics or decompiler behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 18. Changing import handling

Keep true imports, import thunks, undefined externals, and debug-only symbols distinct.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 19. Changing export handling

Preserve symbol provenance and loader-owned function views.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 20. Changing resource lookup

Route through path config and resource roots; do not embed local absolute paths.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 21. Changing utility manifests

Keep manifests deterministic and explain what data is required at runtime.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 22. Changing signature lookup

Keep signature hits evidence-backed and separate from semantic repair.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 23. Changing automation reports

Project `NirBuildStats` and other canonical counters; do not redefine metrics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 24. Changing JSON report contracts

Version or document the contract and keep output deterministic.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 25. Changing CLI output

Keep CLI as a surface; do not fix semantics in formatting code.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 26. Changing CLI command parsing

Separate compatibility shims from command ownership and behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 27. Changing TUI behavior

Preserve backend contracts and avoid UI-specific semantic rules.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 28. Changing Dioxus GUI behavior

Consume shared contracts and avoid duplicate function filtering rules.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 29. Changing AI integration surfaces

Keep AI assistance advisory and preserve deterministic core behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 30. Changing plugin contracts

Keep contracts explicit, stable, and separated from core crate internals.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 31. Changing dynamic-analysis support

Keep dynamic evidence labeled and do not blur it with static facts.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 32. Changing time-travel support

Keep trace-derived facts explicit and reproducible.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 33. Adding a new test fixture

Document provenance, architecture, compiler, and why the fixture is useful.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 34. Adding a regression test

Name the invariant, not just the failing sample.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 35. Updating snapshots

Inspect semantic meaning before accepting changed text.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 36. Investigating benchmark movement

Compare exact rows, artifacts, scores, stdout, stderr, and feature gaps.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 37. Investigating a pass-count change

Trace whether the change is semantic, presentational, telemetry-only, or noise.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 38. Investigating a size change

Inspect line count and byte count together with readability and semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 39. Investigating a CI-only failure

Check OS, LFS resources, workflow inputs, feature flags, and rust version.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 40. Investigating a resource-missing failure

Check LFS pull scope, resource roots, and CLI resource status.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 41. Investigating a panic

Capture command, input, backtrace, crate owner, and minimal reproducer.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 42. Investigating non-determinism

Check map iteration, filesystem order, random seeds, timestamps, and local paths.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 43. Changing dependencies

Justify long-term maintenance value and avoid dependency shortcuts for core semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 44. Consulting Ghidra

Use it for invariants and expected behavior, not copied implementation.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 45. Consulting RetDec

Use it as reference material without creating runtime dependency.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 46. Touching vendor trees

Do not add production links, shell-outs, bindings, or copied shortcuts.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 47. Touching `utils/`

Use existing loaders and manifests instead of bypassing resource configuration.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 48. Writing architecture docs

State owner boundaries and avoid implying surface layers own semantics.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 49. Writing user docs

Prefer commands and observed behavior over aspiration.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 50. Writing troubleshooting docs

Map symptom to first owner and first command.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 51. Writing release notes

Separate features, fixes, quality movement, and known limitations.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 52. Preparing a commit

Stage intended hunks only and keep unrelated dirty work untouched.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 53. Preparing a PR

Lead with behavior change, validation, and residual risk.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 54. Reviewing a PR

Prioritize bugs, regressions, missing tests, and ownership drift.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 55. Refactoring shared code

Keep behavior stable unless the refactor explicitly includes a measured semantic change.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 56. Adding an abstraction

Add it only when it removes real duplication or encodes a real invariant.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 57. Deleting legacy code

Prove the active path no longer depends on it and keep compatibility expectations visible.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 58. Changing telemetry names

Check every consumer and avoid parallel meanings.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 59. Changing public structs

Consider CLI JSON, GUI, automation, and downstream compatibility.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 60. Changing error types

Keep errors typed enough for users and automation to act on.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 61. Changing logging

Keep logs useful for debugging without making tests flaky.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 62. Changing performance-sensitive paths

Measure before and after when the change affects common loops.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 63. Changing memory-heavy paths

Check large binaries and avoid unbounded accumulation.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 64. Changing parser code

Use structured readers and bounds checks rather than ad hoc slicing.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 65. Changing graph algorithms

Prefer explicit graph facts over lexical ordering or sample-specific assumptions.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 66. Changing dataflow analysis

Document convergence, lattice meaning, and budget behavior.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 67. Changing fixed-point loops

Make termination, changed status, and budget behavior inspectable.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 68. Changing type inference

Keep confidence and provenance visible; avoid overconfident names.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 69. Changing ABI handling

Keep architecture and calling convention boundaries explicit.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 70. Changing x86 behavior

Validate exact sample first, then the broader x86/x86-64 family.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 71. Changing non-x86 behavior

Do not regress x86/x86-64 priority while expanding breadth.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 72. Changing docs only

Do not claim semantic improvement from documentation changes.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

### 73. Changing logo or README assets

Keep icon and README logo responsibilities separate.

- Owner: identify the crate and module that owns the behavior before editing.
- Evidence: preserve the command, binary, address, row, fixture, or artifact that motivated the change.
- Coverage: add or update the smallest test that captures the invariant.
- Validation: run the targeted check first, then the relevant crate-level check.
- Regression: compare existing passing rows or smoke lanes when behavior is user-visible.
- Report: separate mechanical change from quality improvement.

## Review Question Bank

Use these questions during code review or before handing off a quality cycle.

- [ ] 01. Which layer owns this behavior?
- [ ] 02. Is the change semantic, presentational, telemetry-only, or documentation-only?
- [ ] 03. What exact row, address, sample, command, or artifact motivated the change?
- [ ] 04. What invariant does the new test encode?
- [ ] 05. Could this patch accidentally special-case one binary?
- [ ] 06. Could this have been fixed lower in the pipeline?
- [ ] 07. Does the printer now infer facts it does not own?
- [ ] 08. Does automation define a metric already owned by `NirBuildStats`?
- [ ] 09. Does the loader fail closed on unsupported input?
- [ ] 10. Does resource lookup go through path configuration?
- [ ] 11. Are vendor files used only as reference material?
- [ ] 12. Is any new dependency justified by a long-term bottleneck?
- [ ] 13. Is output deterministic across machines?
- [ ] 14. Are local absolute paths absent from production code?
- [ ] 15. Does the CLI remain a product surface rather than a semantic owner?
- [ ] 16. Does GUI code consume shared function views?
- [ ] 17. Are errors typed enough to debug?
- [ ] 18. Are logs useful without being test-sensitive?
- [ ] 19. Are large binaries handled without unbounded memory growth?
- [ ] 20. Does the pass pipeline converge with accurate changed flags?
- [ ] 21. Are analysis dependencies declared explicitly?
- [ ] 22. Is fallback output honest when structure is not proven?
- [ ] 23. Are type hints evidence-backed?
- [ ] 24. Are stack slots and registers handled consistently?
- [ ] 25. Is ABI behavior isolated by architecture?
- [ ] 26. Are sample fixtures documented?
- [ ] 27. Were snapshots inspected before acceptance?
- [ ] 28. Were stale caches disabled for semantic benchmark checks?
- [ ] 29. Were row-level artifacts inspected?
- [ ] 30. Did any existing pass row regress?
- [ ] 31. Was the release CLI rebuilt when benchmark validation needed it?
- [ ] 32. Does CI pull the right LFS resources?
- [ ] 33. Are docs updated when public behavior changes?
- [ ] 34. Are limitations stated plainly?

## Maintainer Handoff Template

Use this template when handing off a substantial decompiler-quality change.

- **Problem:**
- **Root cause:**
- **Owner layer:**
- **Implementation summary:**
- **Tests run:**
- **Benchmarks or row checks:**
- **Artifacts inspected:**
- **Quality result:**
- **Regressions checked:**
- **Known risks:**
- **Follow-up work:**
