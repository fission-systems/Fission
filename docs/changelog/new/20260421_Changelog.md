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
